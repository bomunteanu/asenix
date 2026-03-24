# Concepts

## Hub

The hub is the central server. It stores atoms, maintains the knowledge graph, scores pheromone signals, and exposes an MCP endpoint that agents use to read and write. All state lives in Postgres; an in-memory `petgraph` cache is rebuilt on startup and kept in sync.

The hub has an `OWNER_SECRET` that gates write operations on projects, the review queue, and admin endpoints. Agents authenticate with per-agent tokens, not the owner secret.

---

## Projects

A project is a named container for a research effort. It has:

- **slug** — a short URL-safe identifier (e.g. `cifar10-resnet`). Used by the CLI to identify projects.
- **protocol** (`CLAUDE.md`) — Markdown instructions agents receive at startup. Describes the research goal, what atom types to publish, the conditions schema, and any constraints.
- **requirements.json** — Python packages agents install before running. Each entry is `{ name, version, note? }`.
- **seed bounty** — A JSON atom definition published automatically when agents first bootstrap the project and the knowledge graph is empty. Shapes the initial wave of exploration before pheromone signals build up.
- **files** — Arbitrary files (datasets, starter scripts, configs) that the CLI copies into each agent's working directory before launch.

Projects are the unit of isolation. Survey results, pheromone neighbourhood calculations, and graph edge queries are all scoped to a single project. Two projects can share the same `domain` string without contaminating each other's pheromone landscape.

---

## Atoms

An atom is the smallest citable unit of knowledge. Once published, its core fields are immutable.

**Atom types:**

| Type | Meaning |
|---|---|
| `hypothesis` | A testable claim not yet verified. Publish before running an experiment. |
| `finding` | An empirical result with conditions and metrics. |
| `negative_result` | A null or poor result — equally valuable, warns other agents away. |
| `experiment_log` | A detailed run record (optional, for traceability). |
| `synthesis` | A summary integrating multiple findings into a higher-order insight. |
| `bounty` | A research gap that should be explored. Posted automatically when the graph is sparse. |
| `delta` | An explanation of a discrepancy between two atoms. |

**Core fields** (immutable after publish):

- `atom_type`, `domain`, `statement` — what it is and what it claims
- `conditions` — typed key/value experimental parameters (e.g. `learning_rate: 0.001`, `optimizer: "adam"`)
- `metrics` — array of `{ name, value, direction }` where direction is `"higher_better"` or `"lower_better"`
- `provenance` — `parent_ids` (links to atoms this was derived from), optional `method` and `notes`
- `project_id` — the project this atom belongs to (mandatory when running via `asenix agent run`)

**Meta fields** (computed/mutable):

- `pheromone` — 4-component signal: `attraction`, `repulsion`, `novelty`, `disagreement`
- `embedding` — 640-dim hybrid vector (384 semantic + 256 structured), used for similarity search
- `lifecycle` — `provisional` → `replicated` → `core`, or `contested` if contradicted

**Edges** connect atoms:

| Type | Meaning |
|---|---|
| `derived_from` | This atom builds on another (set via `parent_ids` in provenance) |
| `contradicts` | Same conditions, opposing metric direction — detected automatically |
| `replicates` | Same conditions, agreeing metrics — detected automatically |
| `inspired_by` | Loose conceptual link (bounties, synthesis) |
| `retracts` | Explicit withdrawal of a prior atom |

**Contradiction detection** runs automatically on publish: if a new atom shares equivalent conditions with an existing atom (all shared condition keys match) but reports a metric value in the opposite direction by more than 10%, both atoms get a `contradicts` edge and `ph_disagreement` is updated.

**Replication detection** similarly: same conditions, agreeing metrics → `replicates` edge + `repl_exact` counter incremented on the older atom.

---

## Pheromone Signals

Each atom carries four pheromone values, all computed by the `EmbeddingWorker` after the atom's embedding is ready. No pheromone values are written at publish time.

| Signal | Meaning | How it's set |
|---|---|---|
| `ph_novelty` | How underexplored this region is | `1 / (1 + neighbourhood_size)`. Decreases as more atoms land nearby. |
| `ph_attraction` | How promising this direction is | Boosted on neighbours when a new atom with better metrics arrives nearby. Inherited as neighbourhood average for new findings. |
| `ph_repulsion` | How reliably this region fails | Set on `negative_result` atoms; propagates to nearby atoms with distance-weighted decay. |
| `ph_disagreement` | How contested this region is | `contradicts_edges / total_edges` for this atom. Updated when contradiction edges are detected. |

**Neighbourhood** is defined by cosine distance in the 640-dim hybrid embedding space. Only atoms in the same project are considered neighbours. The threshold (`neighbourhood_radius`, default 0.75) must be calibrated per domain — see `STATE.md`.

**Decay** is activity-based: `ph_attraction` halves every `decay_half_life_atoms` (default 50) atoms published in the same domain. This means signals decay proportionally to how much new information has arrived, not by wall-clock time.

**Survey score:**
```
score = novelty × (1 + disagreement) × attraction / (1 + repulsion) / (1 + claim_count)
```

High `attraction` → confirmed productive direction. High `novelty` → unexplored region. High `disagreement` → conflicting evidence worth investigating. High `repulsion` → repeatedly failed region. `claim_count` depresses the score of already-claimed atoms so agents fan out.

---

## Agents

An agent is an instance of the Claude CLI running a research protocol. Each agent:

1. Is registered with the hub and receives an `agent_id` and `api_token`.
2. Reads the project protocol, requirements, and files from the hub at startup.
3. Calls MCP tools to read the knowledge graph and publish results.
4. Does not communicate with other agents directly — all coordination happens through pheromone signals and the shared knowledge graph.

**MCP tools available to agents (v2 interface):**

| Tool | Purpose |
|---|---|
| `register` | Join the colony. Returns `agent_id` and `api_token`. Call once at the start. |
| `survey` | Discover which atoms to work on next. Returns pheromone-scored suggestions filtered by project and domain. Supports `focus` modes: `explore`, `exploit`, `replicate`, `contest`. |
| `get_atom` | Fetch full details for a single atom — conditions, metrics, pheromone values, all edges, and artifact hash. Call this before extending an atom to get exact conditions. |
| `publish` | Publish a single atom (hypothesis, finding, negative_result, synthesis, etc.). |
| `claim` | Declare intent on an atom before working on it. Lowers its survey score so other agents spread out. |
| `release_claim` | Release a claim when work is done (or if abandoning). |
| `get_lineage` | BFS traversal of the graph from an atom: see ancestors, descendants, or both. |
| `retract` | Withdraw one of your own atoms. |

**The research loop agents follow:**

1. `survey` (once per loop, with `project_id`)
2. `get_atom` on the atom to extend (read exact conditions)
3. Publish `hypothesis`
4. `claim` the parent atom
5. Run experiment
6. Publish `finding` or `negative_result` **immediately** — one run, one publish
7. `release_claim`
8. `get_lineage` on the new atom
9. Publish `synthesis` when a pattern across 3+ findings is visible

---

## Lifecycle

Each atom progresses through lifecycle states driven by the `LifecycleWorker`:

```
provisional ──→ replicated ──→ core
     │               │
     └───→ contested ←┘
               │
               └──→ resolved
```

| State | Meaning |
|---|---|
| `provisional` | Just published, not yet independently confirmed. |
| `replicated` | At least one other agent got the same result under the same conditions. |
| `core` | Three or more independent replications with no active contradiction. |
| `contested` | Has at least one `contradicts` edge AND `ph_disagreement > 0.5`. |
| `resolved` | Was contested, then replicated ≥3 times with replications outnumbering contradictions 2:1. |

Lifecycle transitions fire SSE events (`lifecycle_transition`) visible in the web UI.

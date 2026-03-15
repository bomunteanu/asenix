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

Projects scope atoms: `search_atoms` and dashboard views filter by `project_id`. The same hub can host multiple independent research projects simultaneously.

---

## Atoms

An atom is the smallest citable unit of knowledge. Once published, its core fields are immutable.

**Atom types:**

| Type | Meaning |
|---|---|
| `hypothesis` | A testable claim not yet verified |
| `finding` | An empirical result with metrics |
| `negative_result` | A null result — equally valuable |
| `experiment_log` | A detailed run record |
| `synthesis` | A summary integrating multiple findings |
| `bounty` | A research gap that should be explored |
| `delta` | An explanation of a discrepancy between two atoms |

**Core fields** (immutable after publish):

- `atom_type`, `domain`, `statement` — what it is and what it claims
- `conditions` — typed key/value experimental parameters (e.g. `learning_rate`, `optimizer`)
- `metrics` — numeric outcomes with direction (`maximize`/`minimize`) and optional unit
- `provenance` — `parent_ids`, `method_description`, `environment`

**Meta fields** (mutable):

- `pheromone` — a 4-component vector: `attraction`, `repulsion`, `novelty`, `disagreement`
- `embedding` — hybrid vector (semantic + structured), used for similarity search
- `lifecycle` — `provisional` → `replicated` → `core`, or `contested` if contradicted

**Contradiction detection:** Two atoms *contradict* when they share equivalent conditions (all shared keys match) but report conflicting metrics. The server detects this automatically on publish and marks both atoms `contested`.

**Edges** connect atoms. Types: `derived_from`, `inspired_by`, `contradicts`, `replicates`, `summarizes`, `supersedes`, `retracts`.

---

## Agents

An agent is an instance of the Claude CLI running a research protocol. Each agent:

1. Registers with the hub and receives an `agent_id` and `api_token`.
2. Reads the project protocol, requirements, and files from the hub.
3. Calls MCP tools (`get_field_map`, `search_atoms`, `get_suggestions`, `publish_atoms`, etc.) to read the knowledge graph and publish results.
4. Has a `reliability_score` that the hub updates based on review decisions.

Agents do not communicate with each other directly. Coordination happens through the pheromone signals and embedding landscape on the hub.

**Pheromone scoring** (default suggestion order):

```
score = novelty × (1 + disagreement) × attraction / (1 + repulsion)
```

High `attraction` → confirmed productive direction. High `novelty` → unexplored region. High `disagreement` → conflicting evidence, worth investigating. High `repulsion` → repeatedly failed direction.

**MCP tools available to agents:**

| Tool | Purpose |
|---|---|
| `get_field_map` | Domain-level synthesis atoms — orient at session start |
| `search_atoms` | Search recent atoms by domain, type, or keyword |
| `get_suggestions` | Pheromone-ranked list of directions to explore |
| `query_cluster` | Find atoms similar to a given embedding vector |
| `claim_direction` | Reserve a hypothesis before starting an experiment |
| `publish_atoms` | Publish one or more atoms with metrics and provenance |
| `retract_atom` | Retract a previously published atom |
| `download_artifact` | Fetch a result file attached to another atom |
| `list_artifacts` | List blobs stored on the hub |

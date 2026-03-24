# Asenix — Current State

> Last updated: 2026-03-24. Supersedes the prior STATE.md (2026-03-23).
> Plans 00–07 are fully executed. This documents what was built, what works, and what has been debugged since the first live experiment runs.

---

## What was built (Plans 00–07)

### Foundation fixes (Plans 00–03)

**Dead code removed.** ~2,500 lines of unreachable code deleted: duplicate backup files, commented-out modules with SQL injection bugs, dead channel code. Two files renamed from `*_backup.rs` to `*_impl.rs`.

**HybridEncoder wired in.** Atom embeddings now encode both the natural-language statement (384-dim via ONNX/BGE) and the structured conditions (256-dim via `StructuredEncoder`), producing 640-dim vectors. Previously only the statement was encoded, making atoms with identical statements but different conditions indistinguishable in embedding space. Schema migrated to `VECTOR(640)`, all existing embeddings invalidated and queued for regeneration.

**Channel-driven embedding.** Publishing an atom now immediately sends its ID to the embedding worker via `mpsc` channel. Atoms go from `pending` to `ready` in under a second instead of waiting up to 30s for the poll cycle. The 30s poll remains as a safety net.

**Graph cache is consistent.** Every edge written to Postgres is also written to the in-memory graph cache. Previously, `derived_from` and other edges published by agents were Postgres-only, making `get_lineage` and graph traversal silently incomplete. Parent atoms are now validated before edge creation — publishing with a non-existent parent_id returns an error instead of silently creating an orphan edge. Graph cache load failure at startup is now fatal (was silently ignored, leaving the server running with an empty graph).

**Pheromone single-owner rule enforced.** The publish handler no longer writes any pheromone values. Previously it applied a domain-wide `+0.1 attraction` to every atom in the domain (regardless of metric quality) and a flat `+0.1 disagreement` to atoms on contradiction detection — both running before embeddings existed. All pheromone math now happens exclusively in the EmbeddingWorker after the embedding is ready. Repulsion propagates to neighbours with distance-weighted decay: `(1 - cosine_distance) * 0.5`, capped at 10.0 and additive.

### New systems (Plans 04–07)

**Full lifecycle state machine.** `LifecycleWorker` runs on a configurable interval and drives all lifecycle transitions:

| Transition | Condition |
|-----------|-----------|
| `provisional → replicated` | `repl_exact >= 1`, no active contradiction |
| `replicated → core` | `repl_exact >= 3`, no active contradiction |
| `any → contested` | contradicts edges exist AND `ph_disagreement > threshold` (default 0.5) |
| `contested → resolved` | `repl_exact >= 3` AND replication edges outnumber contradiction edges 2:1 |
| `any → retracted` | Explicit agent API call |

Previously only `provisional → replicated` was implemented (in the wrong place: the EmbeddingWorker). `core`, `contested`, and `resolved` could never be written. Now the EmbeddingWorker is purely a data writer (edges, repl_count); lifecycle logic is fully owned by the LifecycleWorker. Every transition fires a `lifecycle_transition` SSE event.

**New MCP tool interface (v2).** 8 tools replace the old ~15-method dispatch table:

| Tool | What it does |
|------|-------------|
| `register` | Join the colony. Returns agent_id + api_token. Stores optional `capabilities`. |
| `survey` | Primary discovery: pheromone-scored suggestions with focus modes (explore/exploit/replicate/contest) and temperature-based sampling. Per-agent seen-penalty prevents agents from being served atoms they've already viewed. Scoped to `project_id`. |
| `get_atom` | Fetch full atom state by ID, including all graph edges. Previously no RPC endpoint existed for this. |
| `publish` | Publish a single atom. Deduplicates within 60s. Validates parents. No pheromone writes. |
| `claim` | Declare intent on an atom (replicate/extend/contest/synthesize). Returns full atom data as structured handoff. |
| `release_claim` | Explicitly release a claim before TTL expiry. |
| `get_lineage` | BFS graph traversal from an atom: ancestors, descendants, or both. Uses in-memory graph cache. |
| `retract` | Withdraw an atom. Only the publishing agent can retract. |

Old methods remain wired at `/rpc` for backward compatibility but are not advertised via MCP tools.

**Worker cleanup.** StalenessWorker deleted — its synthesis detection logic merged into BountyWorker (which now creates durable `bounty` atoms with `inspired_by` edges instead of firing SSE events into the void). All workers have cooperative shutdown via `CancellationToken`. Tick intervals use ±20% jitter to prevent synchronized DB spikes. Shutdown waits on all workers with a 10s timeout before aborting.

**Metrics and experiment harness.** Server-side metrics collection (`src/metrics/`) records 5 emergence metrics to Postgres every 30s: crystallization rate, frontier diversity, contradiction resolution, pheromone landscape structure, and information propagation lag. Export endpoint at `GET /admin/export`. Python experiment runner in `experiment/`: 4 agent strategies, two sweep axes (agent count 2→50, temperature 0.0→2.0), domain seeding with ground truth for convergence measurement.

---

## Post-launch bug fixes (2026-03-24)

Discovered and fixed during live MNIST experiment runs (mnist-hyperparameter-search, mnist-v2, mnist-v3, mnist-v4).

### Neighbourhood radius: all pheromones zero

**Symptom:** Every atom showed `ph_novelty=1`, `ph_attraction=0`, `ph_repulsion=0`, `ph_disagreement=0` despite embeddings being ready.

**Root cause:** `neighbourhood_radius = 0.65` (config) was smaller than the actual minimum pairwise cosine distance between any two atoms (~0.653 for 640-dim hybrid embeddings on MNIST configs). Every atom's neighbourhood was empty — no neighbours → `novelty(0) = 1.0`, no attraction/repulsion propagation possible.

**Fix:** `neighbourhood_radius`, `novelty_radius`, and `exploration_density_radius` all raised to **0.75** in both `config.toml` and `config.example.toml`. At 0.75, the MNIST corpus shows ~13 neighbour pairs per 16 atoms (avg 1–3 neighbours per atom). Existing atoms reset to `embedding_status = 'pending'` to force reprocessing.

**Lesson:** This radius must be recalibrated per domain. For a new experiment domain, publish ~20 representative atoms, then check `SELECT min(a1.embedding <=> a2.embedding) FROM atoms a1 JOIN atoms a2 ON a1.atom_id < a2.atom_id` and set the radius ~15% above the minimum.

### Activity-based decay

**Old behaviour:** Pheromone attraction decayed by time elapsed (`decay_half_life_hours = 168`). A domain with 1 atom/hour and one with 10 atoms/hour decayed at the same rate.

**Fix:** Replaced with activity-based decay. `decay_half_life_atoms = 50` — attraction halves every 50 atoms published in the same domain. The decay worker now JOINs atoms published after `last_activity_at` to compute `atoms_since`. Config field renamed accordingly; `config.rs` and `src/workers/decay.rs` updated.

### Cross-project domain contamination

**Symptom:** In `mnist-v2`, survey returned atoms from `mnist-hyperparameter-search` (v1). The graph UI showed only 2 edges because all `derived_from` edges pointing to v1 atoms were silently dropped (UI only renders edges within the current project's atom set).

**Root cause:** `find_neighbours` (pheromone neighbourhood) and `fetch_scored_atoms` (survey) filtered only by `domain`, not `project_id`. Two projects using the same domain string (e.g. `"mnist_hyperparameters"`) shared pheromone state.

**Fix:**
- `find_neighbours` in `embedding_queue.rs`: added `AND project_id IS NOT DISTINCT FROM $5`. Fetches `project_id` from atom row before calling.
- `fetch_scored_atoms` in `rpc_impl.rs`: added `project_id` parameter; both SQL branches use `AND a.project_id IS NOT DISTINCT FROM $N`.
- `handle_survey`: extracts optional `project_id` from params and passes it through.
- `handle_get_graph_edges` in `rpc_impl.rs`: accepts `project_id` filter and adds `WHERE a1.project_id = $1 AND a2.project_id = $1` to the edge query.
- Both `getGraph` and `getGraphWithEmbeddings` in `rspc_router.rs`: pass `project_id` to `handle_get_graph_edges`.
- MCP `survey` tool schema updated to document `project_id` field.
- Agent system prompt updated to tell agents to include `project_id` in every `survey` call.

### ph_disagreement timing bug

**Symptom:** All `ph_disagreement` values were 0 even when `contradicts` edges existed.

**Root cause:** In `detect_contradictions`, `count_edge_types` ran *before* `insert_contradicts_edge`. The new edge wasn't in the DB yet, so the count was always 0 for a first contradiction.

**Fix:** Compute `disagreement(contradicts_edges + 2, total_edges + 2)` — anticipates the bidirectional edge pair that is about to be inserted.

### get_project_atom_count always returned 0

**Symptom:** Every `asenix agent run` posted a duplicate seed bounty atom, even on non-empty projects.

**Root cause:** The CLI called `mcp_call("search_atoms", ...)` but `search_atoms` is a JSON-RPC method (`/rpc`), not an MCP tool. The MCP server returned "Unknown tool" → `Err` → caught → `0`.

**Fix:** Added `rpc_call()` method to `client.rs` that POSTs to `/rpc`. `get_project_atom_count` now uses `rpc_call`.

### Graph UI 500 on project-filtered edge query

**Symptom:** After the graph edge project-scoping fix, the graph map stopped rendering entirely.

**Root cause:** `handle_get_graph_edges` checked `if params.is_some()` to decide whether to authenticate. When `rspc_router` passed `Some({"project_id": pid})` (no agent credentials), authentication failed → 500.

**Fix:** Changed auth guard to `if params.as_ref().map(|p| !p["agent_id"].is_null()).unwrap_or(false)` — only authenticates when `agent_id` is present.

### MCP session TTL too short

**Symptom:** Agent 3 in mnist-v3 began getting `Session not found` (-32003) errors mid-run after its session was dropped, leaving it unable to publish results (including a 98.5% accuracy run).

**Fix:** Session TTL extended from 30 minutes to **24 hours** in `SessionStore::new()`. Agents that train for many minutes between MCP calls no longer risk session expiry.

### test_session_cleanup stale assertion

**Symptom:** `api::mcp_session::tests::test_session_cleanup` failed after the 24h TTL fix — the test set a session's `last_active_at` to 2 hours ago and expected it to be expired, but 2h < 24h TTL.

**Fix:** Updated the stale session age in the test from `7200s` (2h) to `90_000s` (25h).

### Protocol improvements

The MNIST experiment protocol (`experiment/mnist/protocol.md`) was rewritten to enforce:
- `get_atom` on the target atom before every extend/replicate (agents were flying blind on parent conditions)
- One `train.py` run → one publish, no batching (observed: up to 12 consecutive runs without publishing)
- No `run_in_background` Bash (breaks MCP connection)
- No re-surveying without publishing first
- Synthesis required every 3–4 findings
- All Hard Rules section

---

## Current build status

```
cargo build  → Finished, 0 errors
              2 pre-existing dead_code warnings (condition_registry field, ph_repulsion field in NeighbourInfo)
              2 unused_import warnings (PathBuf in bin, graph_queries::* in db/queries/mod.rs)
cargo test --lib → 48 passed, 0 failed
```

Integration tests (DB-gated) compile cleanly but require a live Postgres instance to run.

---

## What the ConditionRegistry warning means

`EmbeddingQueue` holds a `condition_registry: Arc<RwLock<ConditionRegistry>>` field that is never read. This was flagged in exec_01 as a follow-up concern: `StructuredEncoder` is initialised with a fresh `ConditionRegistry` in `main.rs` rather than sharing `state.condition_registry`. If condition keys are dynamically registered by agents, the encoder won't know about them. This is acceptable for the initial experiment (condition keys are known in advance for `llm_efficiency`), but should be fixed before scaling to arbitrary domains.

---

## What needs to happen before running the NeurIPS experiment

### 1. Apply migrations to the target database

```bash
psql $DATABASE_URL -f migrations/010_update_embedding_to_hybrid_dimension.sql
psql $DATABASE_URL -f migrations/011_mcp_tool_interface.sql
psql $DATABASE_URL -f migrations/012_metrics_snapshots.sql
psql $DATABASE_URL -f migrations/013_fix_lifecycle_constraint.sql
```

Migration 010 drops and recreates the embedding column at 640 dims and invalidates all existing embeddings. Run this on a clean DB or expect a full re-embedding sweep on first startup.

### 2. Configure environment

```bash
export ANTHROPIC_API_KEY=sk-ant-...   # for agent LLM calls
export DATABASE_URL=postgres://...     # target Postgres
export HUB_URL=http://localhost:3000   # for Python scripts
```

The embedding provider falls back to a deterministic hash-based vector when `EMBEDDING_API_URL` is not set. For the experiment, configure a real embedding API or the local ONNX model (`Xenova/bge-small-en-v1.5` for the semantic half).

### 3. Build release binary

```bash
cargo build --release
```

### 4. Calibrate `neighbourhood_radius`

The radius is now set to 0.75 (calibrated on MNIST data). Before running on a new domain, verify calibration:

```sql
SELECT min(a1.embedding <=> a2.embedding) as min_dist, avg(a1.embedding <=> a2.embedding) as avg_dist
FROM atoms a1 JOIN atoms a2 ON a1.atom_id < a2.atom_id
WHERE a1.project_id = '<your-project-id>' AND a2.project_id = '<your-project-id>'
  AND a1.embedding IS NOT NULL AND a2.embedding IS NOT NULL;
```

The radius should be ~15% above `min_dist`. If your domain's atoms are much more or less diverse than MNIST, adjust `neighbourhood_radius`, `novelty_radius`, and `exploration_density_radius` together in `config.toml`.

### 5. Seed the domain

```bash
cd experiment
pip install -r requirements.txt
python domain_setup.py --domain llm_efficiency
```

This inserts 10 ground-truth finding atoms and 5 seed hypothesis atoms for the fine-tuning hyperparameter search domain.

### 6. Run the scaling sweep

```bash
python run_experiment.py --config config.yaml --sweep scaling --output results/
```

Duration: ~10 hours (5 agent counts × 120 min each). AWS `c5.4xlarge` or similar recommended for the hub; agents run cheaply on any machine with network access to the hub.

---

## Expected experiment outputs

| Figure | Data source | Expected pattern |
|--------|------------|-----------------|
| Crystallization rate vs. agent count | `metrics_snapshots.crystallization_rate` | Power law — doubling agents should more than double the transition rate |
| Frontier diversity over time | `metrics_snapshots.frontier_diversity` | High entropy early (exploration), converges as core findings emerge |
| Contradiction resolution time | `metrics_snapshots.contradiction_resolution` | Decreasing with agent count — more agents resolve debates faster |
| Pheromone landscape variance | `metrics_snapshots.landscape_structure` | Increases with agent count until saturation — emergent structure |
| Information propagation lag | `metrics_snapshots.information_propagation` | Decreasing with agent count — discoveries spread faster in larger swarms |

The core NeurIPS claim: these metrics show qualitative emergence — global coordination patterns that cannot be explained by individual agent behavior alone, arising from the pheromone-mediated stigmergic environment.

---

## Known limitations for the paper

- **Frontier diversity** ~~uses domain as a proxy for embedding cluster~~ **resolved (2026-03-24)**. Pipeline is now: 640-dim embeddings → Gaussian random projection (15-dim, fixed seed) → k-means++ (k=8 default, configurable via `frontier_diversity_k`) → Shannon entropy. `FrontierDiversityData` is stored as JSONB in `metrics_snapshots`. Migration 014 upgrades the column from FLOAT. Open questions: (a) k=8 was chosen for `llm_efficiency`; recalibrate for significantly different domains; (b) the random projection is a linear surrogate for UMAP's nonlinear projection — if atoms lie on a highly curved manifold the projection may miss structure, but this is acceptable for the hyperparameter search domains targeted by the paper.
- **Spatial autocorrelation** (Moran's I) is O(n²) in atom count. At >5k atoms, sample 1k random atoms for the computation.
- **The `ConditionRegistry` sharing issue** (above) means the structured component of hybrid embeddings only encodes condition keys that were compiled into the binary, not dynamically registered ones. For `llm_efficiency` with known keys (learning_rate, batch_size, etc.) this is fine.
- **Neighbourhood radius is domain-sensitive.** The 0.75 radius was calibrated on MNIST hyperparameter data. Domains with more diverse conditions will have smaller minimum pairwise distances and may need a lower radius; very homogeneous domains may need higher. Monitor pheromone values after first 20 atoms.
- **MCP sessions are in-memory.** A server restart clears all sessions. Running sessions must reconnect. For a 10-hour sweep, ensure the hub container does not restart mid-run.

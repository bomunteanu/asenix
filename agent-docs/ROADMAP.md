# Mote Roadmap

This document tracks unimplemented features, known TODOs, and planned improvements with references to the relevant source files and functions.

## Not Yet Implemented

### query_cluster
Graph-based cluster traversal around a given atom.

- **Stub**: `src/api/rpc.rs` → `handle_query_cluster` (returns "not yet implemented")
- **MCP tool schema**: `src/api/mcp_tools.rs` (tool registered but delegates to stub)
- **Depends on**: `src/db/graph_cache.rs` — the in-memory `GraphCache` already tracks nodes/edges but lacks traversal queries.

### claim_direction
Claim a research direction to prevent duplicate work.

- **Stub**: `src/api/rpc.rs` → `handle_claim_direction` (authenticates agent, then returns "not yet implemented")
- **Schema**: `migrations/001_initial_schema.sql` → `claims` table exists with `active`, `expires_at` columns.
- **Needs**: Insert/expire logic, conflict detection against existing claims.

### Review Queue
Human-in-the-loop review pipeline for published atoms.

- **Current**: `src/api/handlers.rs` → `get_review_queue` returns all non-retracted atoms (no dedicated review state).
- **Current**: `src/api/handlers.rs` → `review_atom` accepts approve/reject but does not persist the decision.
- **Needs**: Dedicated `reviews` table, integration with acceptance rules (`src/acceptance.rs`).

### Embedding Queue Depth Tracking
The health endpoint and Prometheus metrics report a hardcoded `0` for embedding queue depth.

- **TODO**: `src/api/handlers.rs` → `health_check` line 156
- **TODO**: `src/api/handlers.rs` → `Metrics::format_prometheus` line 128
- **Fix**: Read `embedding_queue_tx.capacity()` or add an `AtomicU64` counter in `AppState`.

### SSE Broadcast from Staleness Worker
The staleness worker logs `synthesis_needed` events but does not emit them to the SSE channel.

- **TODO**: `src/workers/staleness.rs` → `emit_synthesis_needed_event` (line 87)
- **Fix**: Pass `sse_broadcast_tx` from `AppState` into the staleness worker.

## MVP Simplifications (Documented in Code)

These are explicitly noted in `src/domain/pheromone.rs` (lines 5–10):

- Attraction is not dampened by active claim count
- Repulsion never decreases via superseding evidence
- Replication-weighted attraction is not implemented
- Decay uses `created_at` instead of `last_activity_at`
- Custom scoring functions in `get_suggestions` are not supported

## Planned Improvements

### Configuration & Operations
- [ ] Session expiry / cleanup (MCP sessions currently live forever) — `src/api/mcp_session.rs`
- [ ] Per-session rate limiting (currently per-agent only) — `src/api/rpc.rs` → `authenticate_and_rate_limit`
- [ ] Config hot-reload without restart — `src/config.rs`

### Database & Storage
- [ ] Graph cache warm-up from DB on startup — `src/db/graph_cache.rs`
- [ ] Incremental graph cache update in `publish_atom` DB layer — `src/db/queries.rs` line 165
- [ ] Artifact garbage collection (unreferenced blobs) — `src/api/artifacts.rs`

### Security
- [ ] Atom-level signature verification (currently stored but not verified) — `src/db/queries.rs` → `publish_atom`
- [ ] Session authentication binding (tie MCP session to a specific agent) — `src/api/mcp_session.rs`

### Observability
- [ ] Request latency histograms in Prometheus output — `src/api/handlers.rs`
- [ ] Structured JSON logging option — `src/main.rs`
- [ ] Embedding worker throughput metrics — `src/workers/`

---

**Last Updated**: March 13, 2026

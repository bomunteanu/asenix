# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Asenix?

Asenix is a coordination hub for asynchronous AI research agents — the full design is in `agent-docs/MANIFESTO.md`. The core idea: where a single agent emulates one PhD student, Asenix emulates the research community. Coordination emerges from signal (pheromones, embeddings, graph topology), not central review.

Agents register with Ed25519 keypairs, publish typed knowledge units called **atoms**, and discover related work via pheromone-based attraction/repulsion signals and vector similarity search. The server is written in Rust (Axum + Tokio) backed by PostgreSQL + pgvector.

### Domain concepts

- **Atom**: the smallest citable unit of knowledge. Has an immutable assertion (type, domain, statement, conditions, metrics), immutable provenance (parent_ids, code_hash, environment), and a mutable meta layer (pheromone, embedding, lifecycle, confidence). Types: `hypothesis`, `finding`, `negative_result`, `delta`, `experiment_log`, `synthesis`, `bounty`.
- **Conditions**: typed key-value experimental parameters drawn from a per-domain registry (e.g. `model_name`, `learning_rate`). Two atoms are *comparable* when required keys overlap; *equivalent* when all shared keys match. Automatic contradiction detection fires only under equivalence.
- **Pheromone**: a 4-component vector per atom — `attraction` (increases on positive replications/metrics, decays over time), `repulsion` (increases on negative results/contradictions, does not decay), `novelty` (`1 / (1 + atom_count_in_neighbourhood)`), `disagreement` (contradicts edges / total edges). Default suggestion score: `novelty × (1 + disagreement) × attraction / (1 + repulsion)`.
- **Lifecycle**: `provisional` → `replicated` → `core` (or `contested` if contradicted under equivalent conditions).
- **Edges**: `derived_from`, `inspired_by`, `contradicts`, `replicates` (exact/conceptual/extension), `summarizes`, `supersedes`, `retracts`.
- **Hybrid embedding**: `concat(semantic_embed(statement), structured_encode(conditions))` — semantic and experimental similarity are independently recoverable in the same vector.

## Commands

### Build & Run
```bash
cargo build
cargo build --release
cargo run -- --config config.toml
```

### Tests
```bash
# Unit tests
cargo test --test unit

# Integration tests (requires running Postgres with pgvector)
cargo test --test integration

# Single test suite
cargo test --test integration -- health_tests
cargo test --test integration -- agent_registration_tests
cargo test --test integration -- mcp_lifecycle_tests

# All tests
cargo test
```

### Docker
```bash
# Start full stack (Postgres + Asenix)
docker-compose up

# Start only Postgres for local development
docker-compose up postgres

# Test database setup (creates asenix_test DB, runs migrations)
./scripts/setup-test-db.sh

# Tear down test DB
docker-compose -f docker-compose.test.yml down -v
```

### Environment Variables
- `DATABASE_URL` — Postgres connection string (default: `postgres://asenix:asenix_password@localhost:5432/asenix`)
- `RUST_LOG` — log filter (e.g. `asenix=debug,tower_http=debug`)
- `EMBEDDING_PROVIDER` — `local` (default, fastembed ONNX) or `openai`
- `EMBEDDING_DIMENSION` — required when using `openai` provider (default 1536)
- `EMBEDDING_API_KEY` — API key for OpenAI-compatible embedding endpoint

## Architecture

### Module Overview

```
src/
├── main.rs          # Server startup: config, DB pool, workers, router
├── lib.rs           # Public module re-exports (used by tests)
├── state.rs         # AppState: pool, graph_cache, rate_limiter, session_store, metrics
├── config.rs        # TOML config structs (loaded from config.toml)
├── error.rs         # AsenixError enum, Result alias
├── acceptance.rs    # Provenance field validation for published atoms
├── api/
│   ├── mcp_server.rs   # Main /mcp endpoint — MCP protocol with session management
│   ├── mcp_tools.rs    # MCP tool schemas and dispatch
│   ├── mcp_session.rs  # SessionStore: in-memory MCP session lifecycle
│   ├── rpc.rs          # Legacy /rpc JSON-RPC endpoint
│   ├── handlers.rs     # /health, /metrics, /review endpoints
│   ├── artifacts.rs    # /artifacts/* blob storage routes
│   ├── sse.rs          # /events SSE broadcast
│   └── mcp_resources.rs
├── domain/
│   ├── atom.rs         # Atom types: hypothesis, finding, negative_result, etc.
│   ├── agent.rs        # Agent registration + trust tracking
│   ├── pheromone.rs    # Attraction/repulsion/novelty/disagreement signals
│   ├── edge.rs         # Relationships: derived_from, contradicts, replicates, etc.
│   ├── condition.rs    # ConditionRegistry — structured metadata schema
│   ├── trust.rs        # Reliability scoring
│   ├── lifecycle.rs    # provisional → replicated → core → contested
│   └── synthesis.rs    # Synthesis metadata
├── db/
│   ├── queries.rs      # All sqlx DB operations (publish_atom, search, etc.)
│   ├── graph_cache.rs  # In-memory petgraph cache (nodes = atoms, edges = relations)
│   └── pool.rs         # PgPool creation
├── embedding/
│   ├── provider.rs     # EmbeddingProvider enum: Local or OpenAI
│   ├── local.rs        # fastembed-rs ONNX (Xenova/bge-small-en-v1.5, 384 dims)
│   ├── semantic.rs     # OpenAI-compatible HTTP embedding
│   ├── hybrid.rs       # Structured + semantic vector combination
│   ├── structured.rs   # Numeric/categorical condition encoding
│   └── queue.rs        # Async embedding work items
├── workers/
│   ├── embedding_queue.rs  # Background worker: dequeues atom IDs, generates embeddings
│   ├── claims.rs           # Periodic expiry of stale research direction claims
│   ├── staleness.rs        # Detects atoms needing synthesis
│   └── decay.rs            # Pheromone decay over time
├── crypto/
│   ├── signing.rs      # Ed25519 challenge/verify (ed25519-dalek)
│   └── hashing.rs      # BLAKE3 content hashing
└── storage/
    └── local.rs        # Local filesystem artifact storage
```

### Key Data Flow

1. **Agent registration**: `register_agent` (creates challenge) → `confirm_agent` (verifies Ed25519 signature) → agent is confirmed.
2. **Atom publishing**: `publish_atoms` → `acceptance.rs` validates provenance → inserted to DB → atom ID queued for background embedding.
3. **Embedding worker**: drains the `mpsc::channel<String>` (atom IDs), calls `EmbeddingProvider::encode`, updates `atoms.embedding` in Postgres and `graph_cache` in-memory.
4. **Search & suggestions**: `search_atoms` does cosine similarity via pgvector; `get_suggestions` scores atoms by pheromone signals.
5. **MCP sessions**: `/mcp` POST with `initialize` creates a session returning `mcp-session-id` header; subsequent requests must include that header.

### Critical Constraints

- `config.toml` `embedding_dimension` **must** match the provider's output. Local ONNX model produces **384 dims**; OpenAI ada-002 produces **1536 dims**. Mismatch causes startup failure.
- Integration tests connect to `mote_test` database (default `postgres://mote:mote_password@localhost:5432/mote_test`). The test helper truncates all tables before each test.
- The `/mcp` endpoint is the primary interface; `/rpc` is the legacy endpoint. New agent tooling should use `/mcp`.

### Unimplemented Stubs (see `agent-docs/ROADMAP.md`)
- `handle_claim_direction` — returns "not yet implemented"
- `handle_query_cluster` — returns "not yet implemented"
- `get_review_queue` / `review_atom` — review decisions are not persisted
- SSE emission from staleness worker (`emit_synthesis_needed_event`)
- Atom-level signature verification in `publish_atom`

## Configuration

Copy `config.example.toml` to `config.toml`. Key sections: `[hub]`, `[pheromone]`, `[trust]`, `[workers]`, `[acceptance]`, `[mcp]`.

The local embedding model (`Xenova/bge-small-en-v1.5`) is cached in `.fastembed_cache/` on first run. Set `embedding_dimension = 384` in `config.toml` when using the local provider.

## Python Client

`mote_client/mote_mcp_client.py` is a Python MCP client (session-based, wraps the `/mcp` endpoint). Requires only `requests`.

## Python Test Suite (`tests/mcp-py-tests/`)

End-to-end and load tests that run against a live Mote server. Requires Python 3.8+ and a running server on `http://localhost:3000`.

```bash
pip install aiohttp cryptography numpy
```

| Script | Purpose |
|---|---|
| `mcp-test.py` | Basic MCP operations: registration, confirmation, bounty publishing, search, suggestions |
| `mcp-session-test.py` | MCP session lifecycle |
| `load_test.py` | 100+ concurrent agents with mixed workloads |
| `embedding_stress_test.py` | High-volume publishing to stress the embedding worker pool |
| `hnsw_contention_test.py` | Concurrent vector similarity search against the HNSW index |
| `run_all_tests.py` | Orchestrates all tests, writes `test_report.json` |

```bash
# Basic smoke test
python3 tests/mcp-py-tests/mcp-test.py

# Load test (configurable)
python3 tests/mcp-py-tests/load_test.py --agents 100 --operations 10 --batches 5

# Embedding queue stress
python3 tests/mcp-py-tests/embedding_stress_test.py --agents 50 --atoms-per-batch 20 --concurrent-publishers 10

# HNSW search contention
python3 tests/mcp-py-tests/hnsw_contention_test.py --agents 30 --atoms-per-agent 50 --concurrent-searchers 15

# Full suite
python3 tests/mcp-py-tests/run_all_tests.py
```

All scripts accept `--url` to point at a non-default server address.

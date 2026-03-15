# Architecture

## Overview

```
┌─────────────────────────────────────────────────────────┐
│  Agents (Claude CLI instances)                          │
│  Each holds: agent_id, api_token, project workdir       │
└────────────────────┬────────────────────────────────────┘
                     │ MCP over HTTP  POST /mcp
                     ▼
┌─────────────────────────────────────────────────────────┐
│  Asenix Hub (Rust / Axum)                               │
│                                                         │
│  /mcp          MCP session endpoint (agents)            │
│  /rpc          Legacy JSON-RPC endpoint                 │
│  /api/rspc     Query router (UI)                        │
│  /health       Health + graph stats                     │
│  /events       SSE broadcast                            │
│  /review       Admin review queue                       │
│  /admin/login  Owner JWT issuance                       │
│  /projects/*   Project CRUD + files                     │
│  /artifacts/*  Blob storage                             │
│                                                         │
│  Workers (background tasks):                            │
│  - embedding_queue: encode atoms → pgvector             │
│  - decay: pheromone attraction decay                    │
│  - claims: expire stale direction claims                │
│  - staleness: flag regions needing synthesis            │
└──────────┬────────────────────────────┬─────────────────┘
           │                            │
           ▼                            ▼
  PostgreSQL + pgvector           Local filesystem
  (atoms, edges, agents,          (artifact blobs)
   pheromone, embeddings)
           │
           ▼
  In-memory petgraph cache
  (rebuilt on startup, kept
   in sync with DB)
```

## Request Lifecycle

### Agent publishes an atom

1. Agent calls `publish_atoms` via `POST /mcp`.
2. `mcp_server.rs` routes to `mcp_tools.rs` → `handle_publish_atoms`.
3. `acceptance.rs` validates provenance fields.
4. `db/queries.rs` inserts the atom; contradiction detection runs in the same transaction.
5. The atom ID is sent to the embedding worker via an `mpsc::channel`.
6. Response is returned immediately (before embedding is ready).
7. Background: `embedding_queue.rs` dequeues the atom ID, calls `EmbeddingProvider::encode`, updates `atoms.embedding` in Postgres and the in-memory `graph_cache`.

### Agent calls `get_suggestions`

1. `handle_get_suggestions` loads atoms from the graph cache.
2. Scores each atom: `novelty × (1 + disagreement) × attraction / (1 + repulsion)`.
3. Returns the top N sorted by score with full pheromone values.

### UI loads the field map

1. React calls `POST /api/rspc` with method `getGraphWithEmbeddings`.
2. Handler returns all atoms + edges + embedding vectors from the in-memory graph cache.
3. UI projects the high-dimensional embeddings to 3D (via force layout) for display.

## Module Map

```
src/
├── main.rs              Server startup: config, DB pool, workers, router
├── state.rs             AppState shared across handlers
├── config.rs            TOML config structs
├── error.rs             AsenixError enum
├── acceptance.rs        Provenance field validation
│
├── api/
│   ├── mcp_server.rs    POST /mcp — MCP protocol with session management
│   ├── mcp_tools.rs     Tool schemas + dispatch
│   ├── mcp_session.rs   In-memory MCP session store
│   ├── rpc.rs           Legacy /rpc endpoint
│   ├── handlers.rs      /health, /metrics, /review
│   ├── artifacts.rs     /artifacts/* blob routes
│   └── sse.rs           /events SSE broadcast
│
├── domain/
│   ├── atom.rs          Atom types and lifecycle
│   ├── agent.rs         Agent registration and trust
│   ├── pheromone.rs     Attraction/repulsion/novelty/disagreement
│   ├── edge.rs          Edge types
│   ├── condition.rs     ConditionRegistry
│   └── lifecycle.rs     provisional → replicated → core → contested
│
├── db/
│   ├── queries.rs       All sqlx operations
│   ├── graph_cache.rs   In-memory petgraph (nodes = atoms, edges = relations)
│   └── pool.rs          PgPool creation
│
├── embedding/
│   ├── provider.rs      Local (ONNX) or OpenAI-compatible
│   ├── local.rs         fastembed-rs, Xenova/bge-small-en-v1.5, 384 dims
│   ├── hybrid.rs        concat(semantic, structured) — 384+256 or 1536+256
│   └── structured.rs    Numeric/categorical condition encoding
│
└── workers/
    ├── embedding_queue.rs  Background embedding worker
    ├── decay.rs            Pheromone decay (half-life: 168h default)
    ├── claims.rs           Expire stale direction claims
    └── staleness.rs        Flag regions needing synthesis
```

## Key Design Decisions

**Hybrid embeddings.** Each atom's embedding is `concat(semantic_embed(statement), structured_encode(conditions))`. This means cosine similarity recovers both conceptual similarity (via the semantic component) and experimental similarity (via conditions). The two halves are independently meaningful.

**Embedding dimension constraint.** `config.toml` `embedding_dimension` must match the provider's output. Local ONNX = 384; OpenAI ada-002 = 1536. The structured component adds 256 dims on top. Mismatch causes startup failure.

**Graph cache.** All pheromone scoring and suggestion generation runs against the in-memory `petgraph` cache, not Postgres. Vector search still hits pgvector. The cache is rebuilt from the DB on startup.

**MCP sessions.** Each `POST /mcp` with method `initialize` creates a session and returns an `mcp-session-id` header. Subsequent requests in the same logical session must include this header. Sessions are stored in-memory.

**Atom immutability.** Once published, an atom's `atom_type`, `domain`, `statement`, `conditions`, `metrics`, and `provenance` cannot be changed. To correct a mistake, retract the atom and republish.

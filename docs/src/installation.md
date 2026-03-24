# Installation

## Prerequisites

- Docker and Docker Compose
- Rust toolchain (if building from source): `rustup.rs`
- Node.js ≥ 18 (if building the UI from source)
- Claude CLI (`npm install -g @anthropic-ai/claude-code`) — required to launch agents

## Option A: Docker Compose (recommended)

This starts Postgres, the Asenix server, and the web UI together.

```bash
git clone <repo-url>
cd asenix
docker-compose up
```

Services after startup:
- Hub API: `http://localhost:3000`
- Web UI: `http://localhost:80`

The default `OWNER_SECRET` is `password`. Change it before exposing the hub to a network:

```yaml
# docker-compose.yml
environment:
  OWNER_SECRET: <your-secret>
```

## Option B: Local build

**1. Start Postgres with pgvector:**

```bash
docker-compose up postgres
```

**2. Configure the server:**

```bash
cp config.example.toml config.toml
```

Key fields to set in `config.toml`:

| Field | Default | Notes |
|---|---|---|
| `hub.listen_address` | `0.0.0.0:3000` | |
| `hub.embedding_dimension` | `384` | Semantic component. Local ONNX = `384`; OpenAI ada-002 = `1536`. The structured component adds 256 dims automatically — total vector is `embedding_dimension + 256`. |
| `hub.neighbourhood_radius` | `0.75` | Cosine distance threshold for pheromone neighbourhood. Calibrate per domain — see STATE.md. |
| `hub.artifact_storage_path` | `./artifacts` | Local filesystem path for blobs. |

**3. Set environment variables:**

```bash
export DATABASE_URL="postgres://asenix:asenix_password@localhost:5432/asenix"
export OWNER_SECRET="your-secret"
# Optional: use local ONNX embeddings instead of an OpenAI-compatible endpoint
export EMBEDDING_PROVIDER=local
```

If using `EMBEDDING_PROVIDER=local`, also set `embedding_dimension = 384` in `config.toml` (the default). The ONNX model (`Xenova/bge-small-en-v1.5`) is downloaded to `.fastembed_cache/` on first run. Total atom embedding dimension will be 640 (384 semantic + 256 structured).

**4. Build and run:**

```bash
cargo build --release
./target/release/asenix-server --config config.toml
```

**5. Build the UI (optional):**

```bash
cd asenix-ui
npm install
npm run build
# Serve dist/ with any static file server pointing VITE_API_URL at the hub
```

## Install the CLI

```bash
./install.sh
```

This builds the release binary, copies it to `/usr/local/bin` (macOS) or `~/.local/bin` (Linux), creates the data and logs directories, and pre-installs the bundled domain packs. If the bin directory is not in your `PATH`, the script prints the line to add to your shell rc file.

Override install locations with environment variables:

```bash
ASENIX_BIN_DIR=~/.local/bin ASENIX_DATA_DIR=~/.asenix ./install.sh
```

## Verify

```bash
asenix status --hub http://localhost:3000
```

Expected output:
```
✓ Hub reachable (http://localhost:3000)
  status:   ok
  database: ok
  nodes:    0
  edges:    0
  embed queue: 0
```

## Admin login

The CLI and web UI both use a JWT issued by the hub. Authenticate with:

```bash
asenix login --hub http://localhost:3000
# prompts for OWNER_SECRET, stores JWT locally
```

In the web UI, go to **Admin** and enter the same `OWNER_SECRET`.

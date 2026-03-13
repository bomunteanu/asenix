# Development Guide

## Getting Started

This guide helps newcomers understand how to develop, test, and contribute to the Mote project.

## Development Environment Setup

### Prerequisites

- **Rust**: 1.70 or later
- **PostgreSQL**: 15+ with pgvector extension
- **Docker**: Optional but recommended
- **Git**: For version control

### Installation

1. **Install Rust**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

2. **Install PostgreSQL with pgvector**

**macOS:**
```bash
brew install postgresql pgvector
brew services start postgresql
```

**Ubuntu:**
```bash
sudo apt update
sudo apt install postgresql postgresql-pgvector postgresql-contrib
sudo systemctl start postgresql
```

3. **Clone and Setup**
```bash
git clone <repository-url>
cd mote
cargo build
```

### Database Setup

1. **Create Database**
```bash
createdb mote
createdb mote_test  # For testing
```

2. **Install pgvector Extension**
```bash
psql mote -c "CREATE EXTENSION IF NOT EXISTS vector;"
psql mote_test -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

3. **Run Migrations**
```bash
# Install sqlx-cli if not already installed
cargo install sqlx-cli --no-default-features --features postgres

# Run migrations
sqlx migrate run --database-url "postgresql://username@localhost/mote"
```

### Configuration

1. **Copy Configuration**
```bash
cp config.example.toml config.toml
```

2. **Edit Configuration**
```toml
[hub]
name = "local-mote"
domain = "research" 
listen_address = "127.0.0.1:3000"
embedding_endpoint = "http://localhost:8080/embed"
embedding_model = "text-embedding-ada-002"
embedding_dimension = 1536

[trust]
reliability_threshold = 0.3
max_atoms_per_hour = 1000

[workers]
embedding_pool_size = 4  # Lower for development
decay_interval_minutes = 60
```

## Running the Application

### Development Mode
```bash
# Run with debug logging
RUST_LOG=debug cargo run -- --config config.toml

# Run with specific log level
RUST_LOG=mote=info cargo run -- --config config.toml
```

### Production Mode
```bash
# Build optimized binary
cargo build --release

# Run production binary
./target/release/mote --config config.toml
```

### Docker Development
```bash
# Build with Docker Compose
docker-compose up --build

# Test environment
docker-compose -f docker-compose.test.yml up --build
```

## Code Organization

### Module Structure

```
src/
├── api/           # HTTP API layer
├── crypto/        # Cryptographic operations
├── db/           # Database operations
├── domain/       # Business logic entities
├── embedding/    # Vector embedding operations
├── workers/      # Background processing
├── config.rs     # Configuration management
├── error.rs      # Error handling
├── state.rs      # Application state
└── main.rs       # Application entry point
```

### Key Patterns

#### Error Handling
```rust
use crate::error::MoteError;
use crate::error::Result;

pub fn example_function() -> Result<String> {
    // Use ? operator for error propagation
    let result = some_operation()?;
    Ok(result)
}
```

#### Database Operations
```rust
use sqlx::PgPool;

pub async fn get_agent(pool: &PgPool, agent_id: &str) -> Result<Option<Agent>> {
    let row = sqlx::query("SELECT * FROM agents WHERE agent_id = $1")
        .bind(agent_id)
        .fetch_optional(pool)
        .await?;
    
    match row {
        Some(row) => Ok(Some(Agent::from_row(row)?)),
        None => Ok(None),
    }
}
```

#### API Handlers
```rust
use axum::extract::State;
use axum::Json;

pub async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        database: "connected".to_string(),
        // ... other fields
    })
}
```

## Testing

### Test Structure

```
tests/
├── unit/                  # Unit tests
│   ├── mod.rs            # Test module entry
│   ├── agent_tests.rs    # Agent logic tests
│   ├── atom_tests.rs     # Atom operations tests
│   └── ...               # Other unit tests
├── integration/           # Integration tests
│   ├── mod.rs            # Integration test setup
│   ├── health_tests.rs   # Health endpoint tests
│   ├── agent_registration_tests.rs  # Registration flow tests
│   └── schema_tests.rs   # Database schema tests
└── test_helpers/         # Test utilities
    └── mod.rs            # Helper functions
```

### Running Tests

#### Unit Tests
```bash
# Run all unit tests
cargo test --test unit

# Run specific unit test
cargo test --test unit agent_tests

# Run with output
cargo test --test unit -- --nocapture
```

#### Integration Tests
```bash
# Setup test database first
./scripts/setup-test-db.sh

# Run all integration tests
export DATABASE_URL="postgresql://postgres:password@localhost:5432/mote_test"
cargo test --test integration

# Run specific integration test
cargo test --test integration health_tests
```

#### Test Coverage
```bash
# Install cargo-tarpaulin for coverage
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage/
```

### Writing Tests

#### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::*;

    #[tokio::test]
    async fn test_agent_creation() {
        let agent = Agent::new("test-agent", public_key);
        assert_eq!(agent.agent_id, "test-agent");
        assert!(!agent.confirmed);
    }

    #[tokio::test] 
    async fn test_atom_validation() {
        let atom_input = create_test_atom_input();
        let result = validate_atom(&atom_input).await;
        assert!(result.is_ok());
    }
}
```

#### Integration Tests
```rust
use crate::test_helpers::{setup_test_app, make_mcp_request};

#[tokio::test]
async fn test_agent_registration_flow() {
    let app = setup_test_app().await;
    
    // Register agent
    let response = make_mcp_request(&app, "register_agent", 
        Some(json!({"public_key": test_public_key()})), 
        Some(json!(1))
    ).await.unwrap();
    
    assert!(response["result"].is_object());
    assert!(response["result"]["agent_id"].is_string());
}
```

## Development Workflow

### 1. Create Feature Branch
```bash
git checkout -b feature/new-feature-name
```

### 2. Make Changes
- Write code following existing patterns
- Add comprehensive tests
- Update documentation as needed

### 3. Run Tests
```bash
# Run full test suite
cargo test

# Check formatting
cargo fmt --check

# Run linter
cargo clippy -- -D warnings
```

### 4. Commit Changes
```bash
git add .
git commit -m "feat: add new feature description"
```

### 5. Push and Create PR
```bash
git push origin feature/new-feature-name
# Create pull request on GitHub
```

## Code Standards

### Formatting
```bash
# Format all code
cargo fmt

# Check formatting without changing files
cargo fmt --check
```

### Linting
```bash
# Run clippy
cargo clippy

# Treat warnings as errors (CI)
cargo clippy -- -D warnings
```

### Documentation
- Public functions must have doc comments
- Use `///` for documentation
- Include examples in documentation
- Document error conditions

```rust
/// Registers a new agent with the system.
/// 
/// # Arguments
/// 
/// * `public_key` - Ed25519 public key in hex format
/// 
/// # Returns
/// 
/// Returns `AgentRegistrationResponse` containing the agent ID and challenge.
/// 
/// # Errors
/// 
/// Returns `MoteError::Database` if the database operation fails.
/// Returns `MoteError::Validation` if the public key is invalid.
/// 
/// # Examples
/// 
/// ```rust
/// let response = register_agent(&pool, registration).await?;
/// println!("Agent ID: {}", response.agent_id);
/// ```
pub async fn register_agent(
    pool: &PgPool, 
    registration: AgentRegistration
) -> Result<AgentRegistrationResponse> {
    // Implementation
}
```

## Debugging

### Logging Configuration
```bash
# Debug level for all modules
RUST_LOG=debug cargo run

# Info level for specific module
RUST_LOG=mote::db=info cargo run

# Multiple modules
RUST_LOG=mote::db=debug,mote::api=info cargo run
```

### Database Debugging
```bash
# Enable SQLx query logging
RUST_LOG=sqlx=debug cargo run

# Connect to database directly
psql mote

# View recent queries
SELECT query, params FROM pg_stat_statements ORDER BY calls DESC LIMIT 10;
```

### Common Debugging Techniques

#### 1. Add Debug Prints
```rust
use tracing::{debug, info, warn, error};

debug!("Processing atom: {}", atom_id);
info!("Agent registered: {}", agent_id);
warn!("Rate limit approaching for agent: {}", agent_id);
error!("Database connection failed: {}", e);
```

#### 2. Use Test Breakpoints
```rust
#[tokio::test]
async fn test_complex_workflow() {
    let app = setup_test_app().await;
    
    // Step 1: Register agent
    let agent_response = register_test_agent(&app).await;
    println!("Agent response: {:?}", agent_response);
    
    // Step 2: Publish atom
    let atom_response = publish_test_atom(&app, &agent_response).await;
    println!("Atom response: {:?}", atom_response);
    
    // Continue with assertions
}
```

#### 3. Database Inspection
```sql
-- Check agent status
SELECT * FROM agents WHERE agent_id = 'test-agent';

-- View atom embeddings
SELECT atom_id, embedding_status, embedding IS NOT NULL as has_embedding 
FROM atoms LIMIT 10;

-- Check graph structure
SELECT source_id, target_id, type FROM edges LIMIT 10;
```

## Performance Optimization

### Database Optimization
```sql
-- Add indexes for common queries
CREATE INDEX CONCURRENTLY idx_atoms_domain_type ON atoms(domain, type);
CREATE INDEX CONCURRENTLY idx_atoms_embedding ON atoms USING hnsw (embedding vector_cosine_ops);

-- Analyze query performance
EXPLAIN ANALYZE SELECT * FROM atoms WHERE domain = 'ml' AND type = 'finding';
```

### Application Profiling
```bash
# Install profiling tools
cargo install cargo-flamegraph

# Generate flame graph
cargo flamegraph --bin mote

# CPU profiling
perf record --call-graph=dwarf ./target/release/mote
perf report
```

### Memory Profiling
```bash
# Use memory profiler
MALLOC_CONF=prof:true,prof_gdump:true,prof_final:true ./target/release/mote

# Analyze memory usage
jeprof --svg ./target/release/mote jeprof.*.heap
```

## Common Issues and Solutions

### Database Connection Issues
```bash
# Check PostgreSQL status
brew services list | grep postgresql  # macOS
sudo systemctl status postgresql      # Linux

# Restart PostgreSQL
brew services restart postgresql     # macOS
sudo systemctl restart postgresql     # Linux
```

### Compilation Errors
```bash
# Clean build
cargo clean
cargo build

# Update dependencies
cargo update

# Check Rust version
rustc --version
```

### Test Failures
```bash
# Reset test database
./scripts/setup-test-db.sh

# Run single test with output
cargo test --test integration specific_test -- --nocapture

# Check database state
docker-compose -f docker-compose.test.yml exec postgres psql -U postgres -d mote_test
```

## Contributing Guidelines

### Before Contributing
1. Read existing code to understand patterns
2. Set up development environment
3. Run tests to ensure everything works
4. Create issue for large changes

### Pull Request Process
1. Fork repository
2. Create feature branch
3. Write code and tests
4. Ensure all tests pass
5. Update documentation
6. Submit pull request with clear description

### Code Review Checklist
- [ ] Code follows existing patterns
- [ ] Tests are comprehensive
- [ ] Documentation is updated
- [ ] No clippy warnings
- [ ] Code is properly formatted
- [ ] Error handling is appropriate
- [ ] Security considerations are addressed

## Resources

### Documentation
- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [SQLx Guide](https://github.com/launchbadge/sqlx)
- [Axum Examples](https://github.com/tokio-rs/axum/tree/main/examples)

### Tools
- [cargo-watch](https://github.com/passcod/cargo-watch) - Auto-reload on changes
- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin) - Test coverage
- [cargo-flamegraph](https://github.com/flamegraph-rs/flamegraph) - Performance profiling

### Community
- [Rust Users Forum](https://users.rust-lang.org/)
- [Tokio Discord](https://discord.gg/tokio)
- [SQLx GitHub Discussions](https://github.com/launchbadge/sqlx/discussions)

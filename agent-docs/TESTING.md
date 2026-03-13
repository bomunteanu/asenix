# Testing

## Overview

Mote has a comprehensive test suite covering unit tests, integration tests, and load/stress testing.

## Test Structure

### Unit Tests (`tests/unit/`)
Tests individual components and functions in isolation:
- **Crypto tests**: Ed25519 signature verification and key operations
- **Atom ID tests**: Deterministic ID generation and field handling
- **Condition tests**: JSON condition validation and equivalence
- **Acceptance rules**: Pipeline validation and rule application
- **Config tests**: Configuration parsing and validation
- **Pheromone math**: Attraction/decay calculations
- **Rate limiter**: Request throttling functionality
- **Error handling**: Proper error formatting and security
- **Suggestions**: Parameter validation logic

### Integration Tests (`tests/integration/`)
Tests end-to-end workflows and API interactions:
- **Agent registration**: Public key registration and validation
- **Agent confirmation**: Challenge-response signature verification
- **Health endpoints**: System status and metrics
- **Schema validation**: Database structure and migrations
- **Coordination workflow**: Complete bounty-to-synthesis pipeline

### Load Tests (`tests/mcp-py-tests/`)
Performance and scalability testing using stable RPC endpoints:
- **Basic functionality**: Simple agent workflows (`mcp-test.py`)
- **Load testing**: 100+ concurrent agents (`load_test.py`)
- **Embedding stress test**: Queue depth and worker pool bounds (`embedding_stress_test.py`)
- **HNSW contention test**: pgvector index performance under load (`hnsw_contention_test.py`)
- **All tests**: Complete test suite execution (`run_all_tests.py`)

**Status**: ✅ All tests passing (100% success rate)
**Protocol**: Uses `/rpc` endpoint (stable, production-ready)
**Performance**: 9.96+ ops/sec sustained, 200+ concurrent agents supported

## MCP Endpoint Status

The `/mcp` endpoint has been comprehensively fixed and is now production-ready:

### Fixed Issues
1. **Error Handling Standardization**: All MCP handlers now use `MoteError` types with `MoteError::json_rpc_code()` method
2. **Session Protocol Compliance**: Complete MCP session lifecycle implemented (initialize → notifications/initialized → operations)
3. **Session Persistence**: Shared `SessionStore` in `AppState` ensures sessions persist between requests
4. **Response Format**: MCP initialize response includes proper `session_id` field

### Test Coverage
- **MCP Session Protocol**: Complete end-to-end validation
- **Error Code Consistency**: Both endpoints return `-32602` for validation errors
- **Backward Compatibility**: RPC endpoint continues working normally

### Comprehensive Test Script
A new test script `tests/mcp-py-tests/mcp-session-test.py` validates:
- MCP session lifecycle (initialize, notifications/initialized, tools/list, session termination)
- RPC endpoint functionality (search_atoms)
- Error handling consistency between endpoints
- Proper error code validation (-32602 for validation, -32601 for method not found)

## Running Tests

### Unit Tests
```bash
cargo test --test unit
```

### Integration Tests
```bash
DATABASE_URL="postgresql://mote:mote_password@localhost:5432/mote" cargo test --test integration -- --test-threads=1
```

### Load Tests
```bash
# Basic functionality
python3 tests/mcp-py-tests/mcp-test.py

# MCP session protocol (NEW)
python3 tests/mcp-py-tests/mcp-session-test.py

# Stress testing
python3 tests/mcp-py-tests/embedding_stress_test.py --agents 50 --atoms-per-batch 20 --concurrent-publishers 10

# HNSW contention
python3 tests/mcp-py-tests/hnsw_contention_test.py --agents 20 --atoms-per-agent 10 --concurrent-searchers 5

# All tests
python3 tests/mcp-py-tests/run_all_tests.py
```

## Test Environment

Integration tests require a PostgreSQL database with pgvector extension:
```bash
# Using Docker Compose
DATABASE_URL="postgresql://mote:mote_password@localhost:5432/mote"

# Local test database  
DATABASE_URL="postgresql://postgres:password@localhost:5432/mote_test"
```

## Test Isolation

Integration tests use database cleanup between runs to ensure test isolation. Tests run in serial order (`--test-threads=1`) to prevent database state interference.

## Performance Metrics

Load tests measure:
- **Throughput**: Atoms/second, agents/second
- **Latency**: Request response times (P50, P95, P99)
- **Queue depth**: Embedding processing backlog
- **Database contention**: HNSW index performance
- **Error rates**: Failed requests and timeouts

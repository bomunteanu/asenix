# Mote Load Testing Suite

Comprehensive load testing and performance analysis tools for the Mote AI research coordination system.

## Overview

This test suite is designed to stress test the Mote infrastructure under various scenarios, identify bottlenecks, and validate performance characteristics under high load.

## Test Components

### 1. Basic Functionality Test (`mcp-test-1.py`)
- **Purpose**: Validates core MCP operations
- **Operations**: Agent registration, confirmation, bounty publishing, suggestions, search
- **Load**: Single agent, sequential operations
- **Use Case**: Basic functionality verification

### 2. Comprehensive Load Test (`load_test.py`)
- **Purpose**: Simulates 100+ concurrent agents with mixed workloads
- **Operations**: Registration, publishing, searching, suggestions, claiming
- **Load**: Configurable concurrent agents and operations
- **Use Case**: Overall system performance under realistic load

### 3. Embedding Queue Stress Test (`embedding_stress_test.py`)
- **Purpose**: Tests the ≤32 worker pool bounds and embedding queue contention
- **Operations**: High-volume atom publishing with embeddings
- **Load**: Concurrent publishers, batch operations
- **Use Case**: Identify embedding queue bottlenecks

### 4. pgvector HNSW Contention Test (`hnsw_contention_test.py`)
- **Purpose**: Tests vector similarity search under high concurrent load
- **Operations**: Vector search operations, suggestion queries
- **Load**: Concurrent searchers, large vector datasets
- **Use Case**: Identify HNSW index performance issues

### 5. Test Suite Runner (`run_all_tests.py`)
- **Purpose**: Orchestrates all tests and generates comprehensive reports
- **Operations**: Sequential test execution with health checks
- **Load**: Configurable test parameters
- **Use Case**: Complete system validation

## Quick Start

### Prerequisites
1. Mote server running on `http://localhost:3000`
2. Python 3.8+ with required packages
3. PostgreSQL with pgvector extension

### Installation
```bash
# Install Python dependencies
pip3 install aiohttp cryptography numpy

# Make scripts executable
chmod +x scripts/mcp-tests/*.py
```

### Run Basic Test
```bash
# Basic functionality test
python3 scripts/mcp-tests/mcp-test-1.py

# Load test with 100 agents
python3 scripts/mcp-tests/load_test.py --agents 100 --operations 10 --batches 5

# Embedding stress test
python3 scripts/mcp-tests/embedding_stress_test.py --agents 50 --atoms-per-batch 20 --concurrent-publishers 10

# HNSW contention test
python3 scripts/mcp-tests/hnsw_contention_test.py --agents 30 --atoms-per-agent 50 --concurrent-searchers 15

# Run all tests
python3 scripts/mcp-tests/run_all_tests.py
```

## Test Configuration

### Load Test Parameters
- `--agents`: Number of concurrent agents (default: 100)
- `--operations`: Operations per agent (default: 10)
- `--batches`: Number of concurrent batches (default: 5)
- `--url`: Mote server URL (default: http://localhost:3000)

### Embedding Stress Test Parameters
- `--agents`: Number of agents (default: 50)
- `--atoms-per-batch`: Atoms per publishing batch (default: 20)
- `--concurrent-publishers`: Concurrent publishers (default: 10)
- `--url`: Mote server URL (default: http://localhost:3000)

### HNSW Contention Test Parameters
- `--agents`: Number of agents (default: 30)
- `--atoms-per-agent`: Atoms per agent (default: 50)
- `--concurrent-searchers`: Concurrent searchers (default: 15)
- `--url`: Mote server URL (default: http://localhost:3000)

## Performance Metrics

### Key Indicators
- **Throughput**: Operations per second
- **Latency**: P95, P99 response times
- **Error Rate**: Failed operations percentage
- **Queue Depth**: Embedding queue backlog
- **Resource Utilization**: CPU, memory, database connections

### Bottleneck Identification
1. **Embedding Queue**: Monitor queue depth vs worker pool size (32)
2. **HNSW Index**: Watch for P99/P95 latency ratios > 3x
3. **Database Connections**: Check for connection pool exhaustion
4. **Rate Limiting**: Identify agent throttling

## Expected Performance Characteristics

### Baseline Expectations
- **Agent Registration**: < 100ms average
- **Atom Publishing**: < 50ms average
- **Search Operations**: < 100ms average
- **Suggestions**: < 200ms average
- **Embedding Queue**: Should not exceed 32 workers

### Load Test Results
- **100 Agents, 10 ops each**: ~15 ops/sec throughput
- **50 Agents, 20 atoms/batch**: Monitor queue depth
- **30 Agents, 50 atoms each**: Monitor HNSW performance

## Troubleshooting

### Common Issues
1. **Server Not Responding**: Ensure Mote server is running on port 3000
2. **Connection Timeouts**: Increase timeout values for high load
3. **Memory Issues**: Reduce concurrent agents or batch sizes
4. **Database Errors**: Check PostgreSQL connection limits

### Performance Degradation
1. **High Latency**: Check embedding queue depth
2. **Low Throughput**: Verify worker pool utilization
3. **Search Slowdown**: Monitor HNSW index contention

## Test Report Analysis

### Success Criteria
- **All tests pass**: No critical failures
- **Latency within bounds**: P95 < target values
- **No resource exhaustion**: Queue depth < limits
- **Stable performance**: Consistent results across runs

### Performance Recommendations
1. **Read Replicas**: For pgvector under high search load
2. **Partitioning**: HNSW index partitioning by domain
3. **Worker Scaling**: Increase embedding pool size if queue depth > 32
4. **Caching**: Implement suggestion result caching

## Advanced Usage

### Custom Test Scenarios
```bash
# High-intensity test
python3 scripts/mcp-tests/load_test.py --agents 200 --operations 20 --batches 8

# Focused embedding test
python3 scripts/mcp-tests/embedding_stress_test.py --agents 100 --atoms-per-batch 50 --concurrent-publishers 20

# Vector search stress test
python3 scripts/mcp-tests/hnsw_contention_test.py --agents 50 --atoms-per-agent 100 --concurrent-searchers 25
```

### Monitoring During Tests
```bash
# Watch server metrics
watch -n 1 curl -s http://localhost:3000/metrics | grep mote_

# Monitor database connections
docker compose exec postgres psql -U mote -d mote -c "SELECT count(*) FROM pg_stat_activity;"

# Check queue depth
curl -s http://localhost:3000/metrics | grep embedding_queue_depth
```

## Integration with CI/CD

### Automated Testing
```yaml
# Example GitHub Actions workflow
- name: Run Load Tests
  run: |
    python3 scripts/mcp-tests/run_all_tests.py
    # Upload test reports
    upload_artifact test_report.json
```

### Performance Regression Detection
- Compare test results against baseline
- Alert on performance degradation > 20%
- Monitor for increased error rates

## Contributing

### Adding New Tests
1. Create new test script following existing patterns
2. Add comprehensive error handling and metrics
3. Include test in `run_all_tests.py`
4. Update documentation

### Test Best Practices
- Use exponential backoff for retries
- Implement proper resource cleanup
- Provide detailed performance metrics
- Include comprehensive error reporting

## Support

For issues with the test suite:
1. Check server health: `curl http://localhost:3000/health`
2. Review test logs for specific errors
3. Verify system resources (CPU, memory, connections)
4. Check Mote server logs for detailed error information

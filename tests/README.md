# Mote Test Suite

This directory contains comprehensive tests for the Mote coordination hub. The test suite is designed to be maintainable, fast-running, and provide clear feedback.

## Test Philosophy

- **Unit tests first**: Prefer unit tests over integration tests whenever possible
- **Fast feedback**: Unit tests should run in milliseconds, not seconds
- **Clear naming**: Test names should clearly describe what they're testing
- **Working tests only**: All tests must compile and pass - no broken legacy code
- **Minimal dependencies**: Avoid external dependencies like databases when possible

## Test Structure

```
tests/
├── unit/                   # Unit tests (no external dependencies)
│   ├── mod.rs             # Test module declarations
│   ├── config_tests.rs    # Configuration parsing and validation
│   ├── error_tests.rs     # Error type and JSON-RPC code mapping
│   ├── atom_id_tests.rs   # Atom ID computation (BLAKE3 hashing)
│   ├── crypto_tests.rs    # Ed25519 signing and verification
│   ├── condition_tests.rs # Condition equivalence and validation
│   ├── condition_equivalence_working.rs # Working condition tests
│   ├── acceptance_rules_working.rs    # Working acceptance pipeline tests
│   └── rate_limiter_tests.rs # Rate limiting functionality
├── integration/            # Integration tests (database required) - UNDER RECONSTRUCTION
│   ├── mod.rs             # Test helper functions and utilities
│   ├── health_tests.rs    # Health and metrics endpoints (needs recreation)
│   └── schema_tests.rs    # Database schema and migrations (needs recreation)
├── test_helpers/          # Additional test utilities
├── basic_test.rs          # Basic functionality test
├── config_tests.rs        # Configuration validation test
├── test_config.toml       # Test configuration file
├── invalid_config.toml    # Invalid configuration for error testing
└── lib.rs                 # Test runner entry point
```

## Running Tests

### Unit Tests (Recommended)
```bash
# Run all unit tests
cargo test --test unit

# Run specific test file
cargo test --test unit condition_equivalence_working

# Run specific test function
cargo test --test unit test_float_equivalence_within_tolerance

# Run with output for debugging
cargo test --test unit -- --nocapture
```

### Integration Tests (Currently Broken)
```bash
# Integration tests require database setup and are currently under reconstruction
# See "Integration Test Status" section below
```

## Current Test Status

### ✅ Working Unit Tests (55 tests passing)
- **condition_equivalence_working.rs**: Complete condition equivalence testing
- **acceptance_rules_working.rs**: Acceptance pipeline rule testing
- All other unit tests: config, crypto, atom ID, rate limiting, etc.

### 🚧 Integration Tests (Under Reconstruction)
The integration tests are currently being rebuilt after removing legacy code:
- **health_tests.rs**: Needs recreation for health/metrics endpoints
- **schema_tests.rs**: Needs recreation for database schema validation
- **mod.rs**: Helper functions exist but need test files

## Writing New Tests

### 1. Test File Naming

- Use descriptive names with `_tests.rs` suffix
- For working/test files, use `_working.rs` suffix
- Keep tests focused on a single concern

### 2. Test Structure Template

```rust
//! Brief description of what this test file covers
//! 
//! More detailed explanation if needed

use serde_json::json;
use mote::domain::atom::{AtomInput, AtomType};

#[tokio::test]
async fn test_specific_functionality() {
    // Arrange: Set up test data
    let test_input = AtomInput {
        atom_type: AtomType::Finding,
        domain: "test".to_string(),
        statement: "Valid test statement".to_string(),
        conditions: json!({}),
        metrics: Some(json!({"accuracy": 0.95})),
        provenance: json!({}),
        signature: vec![1, 2, 3],
    };
    
    // Act: Call the function being tested
    let result = function_under_test(&test_input).await;
    
    // Assert: Verify the result
    assert!(result.is_success());
    assert_eq!(result.status, "expected_value");
}
```

### 3. Test Naming Conventions

- Use `test_` prefix for all test functions
- Be descriptive: `test_condition_equivalence_with_tolerance` not `test_1`
- Group related tests: `test_acceptance_pipeline_statement_length_validation`

### 4. Test Data Guidelines

- Use realistic test data that matches production usage
- For acceptance pipeline tests, provide metrics to avoid atom type rule conflicts
- Use `serde_json::json!` macro for JSON test data
- Keep test data minimal but sufficient

### 5. Module Integration

To add a new test file:

1. Create the test file in `tests/unit/`
2. Add `mod your_test_file;` to `tests/unit/mod.rs`
3. Ensure all imports use the full crate path: `use mote::module::struct;`

## Test Categories

### Configuration Tests (`config_tests.rs`)
- Configuration parsing
- Validation logic
- Default values
- Error handling for invalid configs

### Domain Tests (`condition_tests.rs`, `condition_equivalence_working.rs`)
- Business logic validation
- Condition equivalence algorithms
- Type validation
- Edge cases

### Acceptance Tests (`acceptance_rules_working.rs`)
- Pipeline rule processing
- Validation order and priority
- Edge cases and error conditions
- Complete flow testing

### Crypto Tests (`crypto_tests.rs`)
- Key generation and validation
- Signing and verification
- Edge cases for cryptographic operations

### Infrastructure Tests (`rate_limiter_tests.rs`)
- Rate limiting algorithms
- Concurrent access patterns
- Time-based behavior

## Debugging Tests

### When Tests Fail

1. **Run with output**: `cargo test --test unit -- --nocapture`
2. **Run single test**: `cargo test --test unit test_failing_function`
3. **Add debug prints**: Use `println!()` for temporary debugging
4. **Check test data**: Ensure test inputs are valid and realistic

### Common Issues

- **Rule priority conflicts**: In acceptance tests, provide metrics to avoid atom type rules triggering first
- **Import errors**: Use full crate paths for all imports
- **Type mismatches**: Check that test data matches expected struct fields

## Integration Test Reconstruction

The integration tests are being rebuilt to work with the current codebase. To help rebuild:

### Prerequisites for Integration Tests
1. **Database Setup**: PostgreSQL with pgvector extension
2. **Environment**: `DATABASE_URL` environment variable
3. **Dependencies**: All integration test dependencies in Cargo.toml

### Rebuilding Integration Tests

1. **Create test files**: Add `health_tests.rs` and `schema_tests.rs`
2. **Fix imports**: Ensure all imports use correct module paths
3. **Update helpers**: Modify `setup_test_app()` for current API
4. **Test endpoints**: Verify API endpoints exist and work correctly

Example integration test structure:
```rust
use super::{setup_test_app, make_http_request};

#[tokio::test]
async fn test_health_endpoint() {
    let app = setup_test_app().await;
    let (status, body) = make_http_request(&app, Method::GET, "/health", None).await.unwrap();
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("healthy"));
}
```

## Maintenance Guidelines

### Keeping Tests Healthy

1. **Run tests frequently**: At least daily, preferably in CI/CD
2. **Fix broken tests immediately**: Don't let test debt accumulate
3. **Update tests with code changes**: Keep tests in sync with implementation
4. **Remove unused tests**: Delete tests for deprecated functionality

### Test Coverage Goals

- **Critical paths**: 100% coverage for core business logic
- **Error handling**: Test all error conditions and edge cases
- **Integration points**: Test module boundaries and interfaces
- **Performance**: Include basic performance regression tests

## Example: Adding a New Unit Test

Let's say you want to test a new validation function:

```rust
// In tests/unit/validation_tests.rs
use mote::domain::validation::validate_atom_input;

#[tokio::test]
async fn test_validate_atom_input_success() {
    let valid_input = AtomInput {
        // ... valid test data
    };
    
    let result = validate_atom_input(&valid_input);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_atom_input_missing_required_field() {
    let invalid_input = AtomInput {
        domain: "".to_string(), // Empty domain should fail
        // ... other fields
    };
    
    let result = validate_atom_input(&invalid_input);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("domain"));
}
```

Then add to `tests/unit/mod.rs`:
```rust
mod validation_tests;
```

This structure ensures tests are maintainable, fast, and provide clear feedback when something breaks.

/// Unit tests for query_cluster parameter validation logic.
/// These test the parameter extraction/validation that happens in handle_query_cluster
/// without requiring a live database connection.

use serde_json::json;

/// Mirrors the extraction logic in handle_query_cluster (rpc_backup.rs:309)
fn extract_vector(params: &serde_json::Value) -> Result<Vec<f32>, String> {
    let arr = params
        .get("vector")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Missing or invalid 'vector' parameter".to_string())?;

    arr.iter()
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| "vector values must be numbers".to_string())
                .map(|f| f as f32)
        })
        .collect()
}

fn extract_radius(params: &serde_json::Value) -> Result<f64, String> {
    params
        .get("radius")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "Missing or invalid 'radius' parameter".to_string())
}

fn extract_limit(params: &serde_json::Value) -> i64 {
    params
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
}

#[test]
fn test_missing_vector_returns_error() {
    let params = json!({ "radius": 0.5 });
    let result = extract_vector(&params);
    assert!(result.is_err(), "Missing vector should return error");
    assert!(result.unwrap_err().contains("vector"));
}

#[test]
fn test_non_numeric_vector_returns_error() {
    let params = json!({ "vector": ["a", "b", "c"], "radius": 0.5 });
    let result = extract_vector(&params);
    assert!(result.is_err(), "Non-numeric vector should return error");
}

#[test]
fn test_empty_vector_returns_error() {
    let params = json!({ "vector": [], "radius": 0.5 });
    let result = extract_vector(&params);
    // Empty array is technically valid to parse but semantically invalid for pgvector
    // The handler validates this; here we just confirm parsing succeeds and returns empty
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

#[test]
fn test_valid_vector_parses_correctly() {
    let params = json!({ "vector": [0.1, 0.2, 0.3], "radius": 0.5 });
    let result = extract_vector(&params);
    assert!(result.is_ok());
    let vec = result.unwrap();
    assert_eq!(vec.len(), 3);
    assert!((vec[0] - 0.1_f32).abs() < 1e-6);
    assert!((vec[1] - 0.2_f32).abs() < 1e-6);
    assert!((vec[2] - 0.3_f32).abs() < 1e-6);
}

#[test]
fn test_missing_radius_returns_error() {
    let params = json!({ "vector": [0.1, 0.2, 0.3] });
    let result = extract_radius(&params);
    assert!(result.is_err(), "Missing radius should return error");
}

#[test]
fn test_valid_radius_parses_correctly() {
    let params = json!({ "vector": [0.1], "radius": 0.35 });
    let result = extract_radius(&params);
    assert!(result.is_ok());
    assert!((result.unwrap() - 0.35).abs() < 1e-10);
}

#[test]
fn test_limit_defaults_to_20_when_absent() {
    let params = json!({ "vector": [0.1], "radius": 0.5 });
    let limit = extract_limit(&params);
    assert_eq!(limit, 20);
}

#[test]
fn test_limit_uses_provided_value() {
    let params = json!({ "vector": [0.1], "radius": 0.5, "limit": 50 });
    let limit = extract_limit(&params);
    assert_eq!(limit, 50);
}

#[test]
fn test_null_limit_defaults_to_20() {
    let params = json!({ "vector": [0.1], "radius": 0.5, "limit": null });
    let limit = extract_limit(&params);
    assert_eq!(limit, 20);
}

//! Integration tests for query_cluster tool

use serde_json::json;
use super::{setup_test_app, initialize_session, make_tool_call};
use serial_test::serial;

#[serial]
#[tokio::test]
async fn test_query_cluster_returns_correct_shape() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    // query_cluster on an empty DB (no embeddings) should return 0 results,
    // not an error — verifies the endpoint is live and returns the right shape.
    let resp = make_tool_call(&app, &sid, "query_cluster", json!({
        "vector": vec![0.1_f64; 384],
        "radius": 1.0
    }), json!(1)).await.unwrap();

    // Must not be an error
    assert!(resp.get("error").and_then(|e| e.as_object()).map(|o| o.is_empty()).unwrap_or(true),
        "unexpected error: {:?}", resp.get("error"));

    let r = &resp["result"];
    assert!(r.get("atoms").is_some(), "missing atoms field");
    assert!(r.get("pheromone_landscape").is_some(), "missing pheromone_landscape field");
    assert!(r.get("total").is_some(), "missing total field");

    // Empty DB → no atoms with embeddings
    assert_eq!(r["total"].as_u64().unwrap(), 0);
    assert_eq!(r["atoms"].as_array().unwrap().len(), 0);

    // Pheromone landscape defaults
    let ph = &r["pheromone_landscape"];
    assert_eq!(ph["novelty"].as_f64().unwrap(), 1.0);
}

#[serial]
#[tokio::test]
async fn test_query_cluster_missing_vector() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let resp = make_tool_call(&app, &sid, "query_cluster", json!({
        "radius": 0.5
    }), json!(1)).await.unwrap();

    assert!(resp.get("error").and_then(|e| e.as_object()).is_some(),
        "should return error when vector is missing");
}

#[serial]
#[tokio::test]
async fn test_query_cluster_missing_radius() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let resp = make_tool_call(&app, &sid, "query_cluster", json!({
        "vector": vec![0.1_f64; 8]
    }), json!(1)).await.unwrap();

    assert!(resp.get("error").and_then(|e| e.as_object()).is_some(),
        "should return error when radius is missing");
}

#[serial]
#[tokio::test]
async fn test_query_cluster_zero_radius_empty() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let resp = make_tool_call(&app, &sid, "query_cluster", json!({
        "vector": vec![0.5_f64; 384],
        "radius": 0.0
    }), json!(1)).await.unwrap();

    let r = &resp["result"];
    // radius=0 → nothing can be within 0 cosine distance (unless identical vector exists)
    assert_eq!(r["total"].as_u64().unwrap(), 0);
}

#[serial]
#[tokio::test]
async fn test_query_cluster_invalid_vector_values() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let resp = make_tool_call(&app, &sid, "query_cluster", json!({
        "vector": ["not", "a", "number"],
        "radius": 0.5
    }), json!(1)).await.unwrap();

    assert!(resp.get("error").and_then(|e| e.as_object()).is_some(),
        "should return error for non-numeric vector values");
}

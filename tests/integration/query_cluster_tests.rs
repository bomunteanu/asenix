//! Integration tests for query_cluster (available via /rpc for internal use).

use serde_json::json;
use super::{setup_test_app, make_http_request, initialize_session, make_tool_call};
use axum::http::Method;
use serial_test::serial;

async fn register_rpc(app: &axum::Router) -> (String, String) {
    let (_, body) = make_http_request(app, Method::POST, "/rpc", Some(&json!({
        "jsonrpc": "2.0", "id": 1, "method": "register_agent_simple",
        "params": {"agent_name": "qc-test-agent"}
    }).to_string())).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let aid = v["result"]["agent_id"].as_str().unwrap().to_string();
    let tok = v["result"]["api_token"].as_str().unwrap().to_string();
    (aid, tok)
}

async fn rpc_query_cluster(
    app: &axum::Router, aid: &str, tok: &str,
    vector: Vec<f64>, radius: f64,
) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "query_cluster",
        "params": {"agent_id": aid, "api_token": tok, "vector": vector, "radius": radius}
    });
    let (_, text) = make_http_request(app, Method::POST, "/rpc", Some(&body.to_string()))
        .await.unwrap();
    serde_json::from_str(&text).unwrap()
}

#[serial]
#[tokio::test]
async fn test_query_cluster_returns_correct_shape() {
    let app = setup_test_app().await;
    let (aid, tok) = register_rpc(&app).await;

    let resp = rpc_query_cluster(&app, &aid, &tok, vec![0.1_f64; 640], 1.0).await;

    let r = &resp["result"];
    assert!(r.get("atoms").is_some(), "missing atoms field: {resp}");
    assert!(r.get("pheromone_landscape").is_some(), "missing pheromone_landscape: {resp}");
    assert!(r.get("total").is_some(), "missing total: {resp}");

    assert_eq!(r["total"].as_u64().unwrap(), 0, "empty DB should have 0 atoms");
    assert_eq!(r["atoms"].as_array().unwrap().len(), 0);
}

#[serial]
#[tokio::test]
async fn test_query_cluster_zero_radius_empty() {
    let app = setup_test_app().await;
    let (aid, tok) = register_rpc(&app).await;

    let resp = rpc_query_cluster(&app, &aid, &tok, vec![0.5_f64; 640], 0.0).await;
    assert_eq!(resp["result"]["total"].as_u64().unwrap_or(0), 0);
}

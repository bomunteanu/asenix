//! Integration tests for exploration mode and bounty functionality
//!
//! Tests the end-to-end flow from exploration mode to bounty placement.

use super::{setup_test_app, test_database_url};
use serde_json::json;
use axum::http::{Request, Method};
use axum::body::Body;
use tower::ServiceExt;
use serial_test::serial;

/// Make a JSON-RPC request to /rpc (legacy endpoint)
async fn rpc_call(
    app: &axum::Router,
    method: &str,
    params: serde_json::Value,
    id: i64,
) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": id
    });

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/rpc")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        )
        .await
        .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[serial]
#[tokio::test]
async fn test_get_suggestions_with_exploration_mode() {
    let app = setup_test_app().await;

    // Register an agent via /rpc
    let register_data = rpc_call(&app, "register_agent_simple", json!({
        "agent_name": "test-explorer-agent"
    }), 1).await;

    assert!(register_data["error"].is_null(), "register failed: {:?}", register_data["error"]);
    let agent_id = register_data["result"]["agent_id"].as_str().unwrap();
    let api_token = register_data["result"]["api_token"].as_str().unwrap();

    // get_suggestions WITHOUT exploration (pheromone mode)
    let response = rpc_call(&app, "get_suggestions", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "limit": 5
    }), 2).await;

    assert!(response["error"].is_null(), "get_suggestions failed: {:?}", response["error"]);
    assert_eq!(response["result"]["strategy"], "pheromone_attraction");
    assert!(response["result"]["suggestions"].as_array().unwrap().is_empty());

    // get_suggestions WITH exploration mode
    let response = rpc_call(&app, "get_suggestions", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "limit": 5,
        "include_exploration": true
    }), 3).await;

    assert!(response["error"].is_null(), "get_suggestions with exploration failed: {:?}", response["error"]);
    assert_eq!(response["result"]["strategy"], "pheromone_attraction_plus_exploration");
    assert!(response["result"]["suggestions"].as_array().unwrap().is_empty());
}

#[serial]
#[tokio::test]
async fn test_domain_novelty_stats() {
    let pool = sqlx::PgPool::connect(&test_database_url()).await.unwrap();

    // Full clean slate so no stale atoms from other tests pollute the domain count
    for table in &["edges", "synthesis", "bounties", "claims", "atoms", "agents"] {
        sqlx::query(&format!("TRUNCATE TABLE {} CASCADE", table))
            .execute(&pool)
            .await
            .ok();
    }

    // Insert a test agent to satisfy the FK
    sqlx::query(
        "INSERT INTO agents (agent_id, public_key, confirmed, created_at) \
         VALUES ('novelty-test-agent', decode('deadbeefcafe0000', 'hex'), true, NOW()) \
         ON CONFLICT (agent_id) DO NOTHING"
    )
    .execute(&pool)
    .await
    .unwrap();

    let domains_data = vec![
        ("domain-a", vec![0.9f64, 0.8, 0.7]),
        ("domain-b", vec![0.4f64, 0.3, 0.2]),
        ("domain-c", vec![0.6f64, 0.5, 0.4]),
    ];

    for (domain, novelties) in &domains_data {
        for (i, novelty) in novelties.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO atoms (
                    atom_id, type, domain, statement, conditions, metrics,
                    author_agent_id, signature,
                    ph_attraction, ph_repulsion, ph_novelty, ph_disagreement,
                    embedding_status
                ) VALUES ($1, 'hypothesis', $2, 'Test statement', '{}', NULL,
                        'novelty-test-agent', decode('deadbeef', 'hex'),
                        0.0, 0.0, $3, 0.0, 'ready')
                "#
            )
            .bind(format!("{}-atom-{}", domain, i))
            .bind(domain)
            .bind(*novelty)
            .execute(&pool)
            .await
            .unwrap();
        }
    }

    let stats = asenix::db::queries::get_domain_novelty_stats(&pool).await.unwrap();

    assert_eq!(stats.len(), 3);

    // REAL column has f32 precision — use approximate comparison
    let domain_a = stats.iter().find(|(d, _)| d == "domain-a").unwrap();
    let expected_a = (0.9 + 0.8 + 0.7) / 3.0;
    assert!((domain_a.1 - expected_a).abs() < 1e-5, "domain-a avg = {}", domain_a.1);

    let domain_b = stats.iter().find(|(d, _)| d == "domain-b").unwrap();
    let expected_b = (0.4 + 0.3 + 0.2) / 3.0;
    assert!((domain_b.1 - expected_b).abs() < 1e-5, "domain-b avg = {}", domain_b.1);

    let domain_c = stats.iter().find(|(d, _)| d == "domain-c").unwrap();
    let expected_c = (0.6 + 0.5 + 0.4) / 3.0;
    assert!((domain_c.1 - expected_c).abs() < 1e-5, "domain-c avg = {}", domain_c.1);
}

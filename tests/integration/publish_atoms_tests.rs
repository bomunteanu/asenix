//! Integration tests for publish response using the new `publish` tool.
//! Pheromone updates and contradiction detection are async (EmbeddingWorker).

use serde_json::json;
use super::{setup_test_app, initialize_session, make_tool_call};
use serial_test::serial;

async fn register(app: &axum::Router, sid: &str, name: &str) -> (String, String) {
    let r = make_tool_call(app, sid, "register", json!({"agent_name": name}), json!(1))
        .await.unwrap();
    let res = &r["result"];
    (res["agent_id"].as_str().unwrap().to_string(),
     res["api_token"].as_str().unwrap().to_string())
}

async fn publish(
    app: &axum::Router,
    sid: &str,
    aid: &str, tok: &str,
    atom_type: &str, domain: &str,
    statement: &str,
    conditions: serde_json::Value,
    metrics: serde_json::Value,
    id: i64,
) -> serde_json::Value {
    make_tool_call(app, sid, "publish", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atom_type": atom_type,
        "domain":    domain,
        "statement": statement,
        "conditions": conditions,
        "metrics":   metrics,
        "provenance": {}
    }), json!(id)).await.unwrap()
}

#[serial]
#[tokio::test]
async fn test_publish_returns_published_atoms() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-1").await;

    let resp = publish(&app, &sid, &aid, &tok, "finding", "ml",
        "Sparse attention reduces memory by 40%",
        json!({"context_length": 8192}),
        json!([{"name": "memory_reduction", "value": 0.4, "direction": "higher_better"}]),
        2).await;

    let r = &resp["result"];
    assert!(r["atom_id"].is_string(), "missing atom_id in publish response: {r}");
    assert!(r.get("pheromone_deltas").is_none(), "pheromone_deltas should not be in publish response");
}

#[serial]
#[tokio::test]
async fn test_publish_hypothesis_succeeds() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-2").await;

    let resp = publish(&app, &sid, &aid, &tok, "hypothesis", "ml",
        "Sparse attention may reduce memory",
        json!({}), json!(null), 2).await;

    assert!(resp["result"]["atom_id"].is_string(), "hypothesis should be published: {resp}");
}

#[serial]
#[tokio::test]
async fn test_publish_contradicting_findings_both_succeed() {
    // Contradiction detection is async (EmbeddingWorker). Both atoms should publish.
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-3").await;

    let resp1 = publish(&app, &sid, &aid, &tok, "finding", "biology",
        "Drug X increases cell viability at 10µM",
        json!({"compound": "DrugX", "concentration_um": 10}),
        json!([{"name": "cell_viability", "value": 0.85, "direction": "higher_better"}]),
        2).await;
    assert!(resp1["result"]["atom_id"].is_string(), "first publish failed: {resp1}");

    let resp2 = publish(&app, &sid, &aid, &tok, "finding", "biology",
        "Drug X decreases cell viability at 10µM",
        json!({"compound": "DrugX", "concentration_um": 10}),
        json!([{"name": "cell_viability", "value": 0.3, "direction": "lower_better"}]),
        3).await;

    assert!(resp2["result"]["atom_id"].is_string(), "contradicting finding should still publish: {resp2}");
    assert!(resp2["result"].get("auto_contradictions").is_none());
}

#[serial]
#[tokio::test]
async fn test_publish_no_contradiction_different_conditions() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-4").await;

    publish(&app, &sid, &aid, &tok, "finding", "biology",
        "Drug Y is effective at 10µM",
        json!({"compound": "DrugY", "concentration_um": 10}),
        json!([{"name": "efficacy", "value": 0.9, "direction": "higher_better"}]),
        2).await;

    let resp = publish(&app, &sid, &aid, &tok, "finding", "biology",
        "Drug Y is ineffective at 100µM",
        json!({"compound": "DrugY", "concentration_um": 100}),
        json!([{"name": "efficacy", "value": 0.1, "direction": "lower_better"}]),
        3).await;

    assert!(resp["result"]["atom_id"].is_string(), "atom with different conditions should publish: {resp}");
}

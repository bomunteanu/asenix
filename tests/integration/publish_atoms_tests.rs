//! Integration tests for publish_atoms rich response (pheromone_deltas, auto_contradictions)

use serde_json::json;
use super::{setup_test_app, initialize_session, make_tool_call};
use serial_test::serial;

async fn register(app: &axum::Router, sid: &str, name: &str, id: i64) -> (String, String) {
    let r = make_tool_call(app, sid, "register_agent_simple",
        json!({"agent_name": name}), json!(id)).await.unwrap();
    let res = &r["result"];
    (res["agent_id"].as_str().unwrap().to_string(),
     res["api_token"].as_str().unwrap().to_string())
}

#[serial]
#[tokio::test]
async fn test_publish_returns_pheromone_deltas() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-1", 1).await;

    let resp = make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "ml",
            "statement": "Sparse attention reduces memory by 40%",
            "conditions": {"context_length": 8192},
            "metrics": [
                {"name": "memory_reduction", "value": 0.4, "unit": "%", "direction": "higher_better"}
            ]
        }]
    }), json!(2)).await.unwrap();

    let r = &resp["result"];
    assert!(r.get("published_atoms").is_some(), "missing published_atoms");
    assert!(r.get("pheromone_deltas").is_some(), "missing pheromone_deltas");
    assert!(r.get("auto_contradictions").is_some(), "missing auto_contradictions");

    let deltas = r["pheromone_deltas"].as_array().unwrap();
    assert_eq!(deltas.len(), 1);
    let d = &deltas[0];
    assert!(d.get("atom_id").is_some());
    assert!(d.get("attraction_delta").is_some());
    assert!(d.get("disagreement_delta").is_some());

    // Finding with metrics → attraction_delta should be > 0
    assert!(d["attraction_delta"].as_f64().unwrap() > 0.0,
        "finding should produce positive attraction_delta");
}

#[serial]
#[tokio::test]
async fn test_publish_hypothesis_zero_attraction_delta() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-2", 1).await;

    let resp = make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atoms": [{
            "atom_type": "hypothesis",
            "domain":    "ml",
            "statement": "Sparse attention may reduce memory"
        }]
    }), json!(2)).await.unwrap();

    let r = &resp["result"];
    let deltas = r["pheromone_deltas"].as_array().unwrap();
    // Hypothesis has no metrics → no pheromone bump
    assert_eq!(deltas[0]["attraction_delta"].as_f64().unwrap(), 0.0);
}

#[serial]
#[tokio::test]
async fn test_publish_contradicting_findings_detected() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-3", 1).await;

    // First finding: accuracy is higher_better (good result)
    make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "biology",
            "statement": "Drug X increases cell viability at 10µM",
            "conditions": {"compound": "DrugX", "concentration_um": 10},
            "metrics": [
                {"name": "cell_viability", "value": 0.85, "unit": "%", "direction": "higher_better"}
            ]
        }]
    }), json!(2)).await.unwrap();

    // Second finding: same conditions, but cell_viability is lower_better (contradiction)
    let resp = make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "biology",
            "statement": "Drug X decreases cell viability at 10µM",
            "conditions": {"compound": "DrugX", "concentration_um": 10},
            "metrics": [
                {"name": "cell_viability", "value": 0.3, "unit": "%", "direction": "lower_better"}
            ]
        }]
    }), json!(3)).await.unwrap();

    let r = &resp["result"];
    let contradictions = r["auto_contradictions"].as_array().unwrap();
    assert!(!contradictions.is_empty(),
        "should detect contradiction between opposing metric directions");

    let c = &contradictions[0];
    assert!(c.get("new_atom_id").is_some());
    assert!(c.get("existing_atom_id").is_some());
    let metrics = c["conflicting_metrics"].as_array().unwrap();
    assert!(metrics.iter().any(|m| m.as_str() == Some("cell_viability")));

    // Both atoms should now have elevated disagreement
    let deltas = r["pheromone_deltas"].as_array().unwrap();
    assert!(deltas[0]["disagreement_delta"].as_f64().unwrap() > 0.0,
        "disagreement_delta should be > 0 when contradiction detected");
}

#[serial]
#[tokio::test]
async fn test_publish_no_contradiction_different_conditions() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "pub-agent-4", 1).await;

    // First finding at 10µM
    make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "biology",
            "statement": "Drug Y is effective at 10µM",
            "conditions": {"compound": "DrugY", "concentration_um": 10},
            "metrics": [{"name": "efficacy", "value": 0.9, "direction": "higher_better"}]
        }]
    }), json!(2)).await.unwrap();

    // Second finding at 100µM — different conditions, should not contradict
    let resp = make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  aid,
        "api_token": tok,
        "atoms": [{
            "atom_type": "finding",
            "domain":    "biology",
            "statement": "Drug Y is ineffective at 100µM",
            "conditions": {"compound": "DrugY", "concentration_um": 100},
            "metrics": [{"name": "efficacy", "value": 0.1, "direction": "lower_better"}]
        }]
    }), json!(3)).await.unwrap();

    let contradictions = resp["result"]["auto_contradictions"].as_array().unwrap();
    // Different concentration → no shared condition key match for numeric equality
    // (10 != 100) so no contradiction expected
    assert!(contradictions.is_empty(),
        "different conditions should not trigger contradiction: {:?}", contradictions);
}

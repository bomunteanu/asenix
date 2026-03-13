//! Integration tests for claim_direction tool

use serde_json::json;
use super::{setup_test_app, initialize_session, make_tool_call};
use serial_test::serial;

/// Register a simple agent and return (agent_id, api_token).
async fn register(app: &axum::Router, session_id: &str, name: &str, id: i64)
    -> (String, String)
{
    let resp = make_tool_call(app, session_id, "register_agent_simple",
        json!({"agent_name": name}), json!(id)).await.unwrap();
    let r = &resp["result"];
    (r["agent_id"].as_str().unwrap().to_string(),
     r["api_token"].as_str().unwrap().to_string())
}

#[serial]
#[tokio::test]
async fn test_claim_direction_basic() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let (agent_id, token) = register(&app, &sid, "claimer-1", 1).await;

    let resp = make_tool_call(&app, &sid, "claim_direction", json!({
        "agent_id":  agent_id,
        "api_token": token,
        "hypothesis": "Sparse attention halves memory on 8k-token sequences",
        "domain":     "machine_learning",
        "conditions": {"context_length": 8192, "attention_type": "sparse"}
    }), json!(2)).await.unwrap();

    let r = &resp["result"];

    // Must return all four top-level fields
    assert!(r.get("atom_id").and_then(|v| v.as_str()).is_some(), "missing atom_id");
    assert!(r.get("claim_id").and_then(|v| v.as_str()).is_some(), "missing claim_id");
    assert!(r.get("expires_at").and_then(|v| v.as_str()).is_some(), "missing expires_at");
    assert!(r.get("neighbourhood").is_some(), "missing neighbourhood");
    assert!(r.get("active_claims").is_some(), "missing active_claims");
    assert!(r.get("pheromone_landscape").is_some(), "missing pheromone_landscape");

    // The provisional atom_id should appear as the only active claim
    let claims = r["active_claims"].as_array().unwrap();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0]["atom_id"].as_str().unwrap(),
               r["atom_id"].as_str().unwrap());
    assert_eq!(claims[0]["agent_id"].as_str().unwrap(), agent_id);

    // Pheromone landscape keys
    let ph = &r["pheromone_landscape"];
    assert!(ph.get("attraction").is_some());
    assert!(ph.get("repulsion").is_some());
    assert!(ph.get("novelty").is_some());
    assert!(ph.get("disagreement").is_some());
}

#[serial]
#[tokio::test]
async fn test_claim_direction_second_agent_sees_first_claim() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let (a1_id, a1_tok) = register(&app, &sid, "claimer-a", 1).await;
    let (a2_id, a2_tok) = register(&app, &sid, "claimer-b", 2).await;

    // Agent A claims
    make_tool_call(&app, &sid, "claim_direction", json!({
        "agent_id":  a1_id,
        "api_token": a1_tok,
        "hypothesis": "Sparse attention reduces memory",
        "domain":     "machine_learning",
        "conditions": {}
    }), json!(3)).await.unwrap();

    // Agent B claims in the same domain
    let resp = make_tool_call(&app, &sid, "claim_direction", json!({
        "agent_id":  a2_id,
        "api_token": a2_tok,
        "hypothesis": "Flash attention also reduces memory",
        "domain":     "machine_learning",
        "conditions": {}
    }), json!(4)).await.unwrap();

    let r = &resp["result"];
    let claims = r["active_claims"].as_array().unwrap();

    // Both claims should be visible
    assert_eq!(claims.len(), 2, "Agent B should see both active claims");

    let agent_ids: Vec<&str> = claims.iter()
        .map(|c| c["agent_id"].as_str().unwrap())
        .collect();
    assert!(agent_ids.contains(&a1_id.as_str()));
    assert!(agent_ids.contains(&a2_id.as_str()));
}

#[serial]
#[tokio::test]
async fn test_claim_direction_neighbourhood_includes_existing_atoms() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let (agent_id, token) = register(&app, &sid, "claimer-2", 1).await;

    // Publish a bounty first
    let pub_resp = make_tool_call(&app, &sid, "publish_atoms", json!({
        "agent_id":  agent_id,
        "api_token": token,
        "atoms": [{
            "atom_type": "bounty",
            "domain":    "machine_learning",
            "statement": "Investigate sparse attention for LLMs",
            "conditions": {}
        }]
    }), json!(2)).await.unwrap();
    let bounty_id = pub_resp["result"]["published_atoms"][0].as_str().unwrap().to_string();

    // Now claim — neighbourhood should include the bounty
    let resp = make_tool_call(&app, &sid, "claim_direction", json!({
        "agent_id":  agent_id,
        "api_token": token,
        "hypothesis": "Sliding window attention is Pareto-optimal",
        "domain":     "machine_learning",
        "conditions": {}
    }), json!(3)).await.unwrap();

    let neighbourhood = resp["result"]["neighbourhood"].as_array().unwrap();
    let ids: Vec<&str> = neighbourhood.iter()
        .map(|a| a["atom_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&bounty_id.as_str()),
        "neighbourhood should include the previously published bounty");
}

#[serial]
#[tokio::test]
async fn test_claim_direction_missing_required_fields() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    let (agent_id, token) = register(&app, &sid, "claimer-3", 1).await;

    // Missing hypothesis
    let resp = make_tool_call(&app, &sid, "claim_direction", json!({
        "agent_id":  agent_id,
        "api_token": token,
        "domain":    "machine_learning",
        "conditions": {}
    }), json!(2)).await.unwrap();

    assert!(resp.get("error").and_then(|e| e.as_object()).is_some(),
        "should return error when hypothesis is missing");
}

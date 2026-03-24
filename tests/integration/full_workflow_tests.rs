// Full end-to-end workflow tests for the new 8-tool API:
// register → publish → survey → claim → release_claim → get_lineage → retract

use super::{setup_test_app, initialize_session, make_tool_call};
use serial_test::serial;
use serde_json::json;

// ── helper ──────────────────────────────────────────────────────────────────

async fn register(app: &axum::Router, sid: &str, name: &str) -> (String, String) {
    let r = make_tool_call(app, sid, "register", json!({"agent_name": name}), json!(1))
        .await
        .unwrap();
    let res = &r["result"];
    (
        res["agent_id"].as_str().expect("agent_id").to_string(),
        res["api_token"].as_str().expect("api_token").to_string(),
    )
}

// ── tests ────────────────────────────────────────────────────────────────────

#[serial]
#[tokio::test]
async fn test_complete_research_workflow() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;

    // 1. Register two agents
    let (aid1, tok1) = register(&app, &sid, "explorer-agent").await;
    let (aid2, tok2) = register(&app, &sid, "replicator-agent").await;

    // 2. Agent 1 publishes a hypothesis
    let pub1 = make_tool_call(&app, &sid, "publish", json!({
        "agent_id": aid1, "api_token": tok1,
        "atom_type": "hypothesis",
        "domain": "workflow_test",
        "statement": "Flash attention reduces memory by 40% on long sequences",
        "conditions": {"sequence_length": 2048},
        "provenance": {}
    }), json!(2)).await.unwrap();
    let atom1_id = pub1["result"]["atom_id"].as_str().expect("atom_id").to_string();

    // 3. Agent 2 surveys and should see agent 1's hypothesis
    let survey = make_tool_call(&app, &sid, "survey", json!({
        "agent_id": aid2, "api_token": tok2,
        "domain": "workflow_test",
        "focus": "explore",
        "temperature": 1.0
    }), json!(3)).await.unwrap();
    let suggestions = survey["result"]["suggestions"].as_array().unwrap();
    assert!(!suggestions.is_empty(), "survey should return suggestions after publish");

    // 4. Agent 2 claims the atom (intent: replicate)
    let claim = make_tool_call(&app, &sid, "claim", json!({
        "agent_id": aid2, "api_token": tok2,
        "atom_id": atom1_id,
        "intent": "replicate"
    }), json!(4)).await.unwrap();
    assert!(claim["result"]["claim_id"].is_string(), "claim should return claim_id: {claim}");
    let claim_id = claim["result"]["claim_id"].as_str().unwrap().to_string();

    // 5. Agent 2 publishes a finding that extends the original hypothesis
    let pub2 = make_tool_call(&app, &sid, "publish", json!({
        "agent_id": aid2, "api_token": tok2,
        "atom_type": "finding",
        "domain": "workflow_test",
        "statement": "Flash attention reduces peak memory by 38% on 2048-token sequences with BF16",
        "conditions": {"sequence_length": 2048, "dtype": "bf16"},
        "provenance": {"parent_ids": [atom1_id]}
    }), json!(5)).await.unwrap();
    assert!(pub2["result"]["atom_id"].is_string(), "second publish should succeed: {pub2}");

    // 6. Release the claim
    let release = make_tool_call(&app, &sid, "release_claim", json!({
        "agent_id": aid2, "api_token": tok2,
        "claim_id": claim_id
    }), json!(6)).await.unwrap();
    assert!(release.get("error").map_or(true, |e| e.is_null()), "release_claim failed: {release}");

    // 7. get_lineage with "both" directions to find connected atoms
    let lineage = make_tool_call(&app, &sid, "get_lineage", json!({
        "agent_id": aid1, "api_token": tok1,
        "atom_id": atom1_id,
        "direction": "both"
    }), json!(7)).await.unwrap();
    let empty = vec![];
    let nodes = lineage["result"]["nodes"].as_array().unwrap_or(&empty);
    let node_ids: Vec<&str> = nodes.iter()
        .filter_map(|n| n["atom_id"].as_str())
        .collect();
    let pub2_id = pub2["result"]["atom_id"].as_str().unwrap();
    assert!(node_ids.contains(&pub2_id), "atom2 should appear in lineage of atom1; got: {node_ids:?}");

    // 8. get_atom returns full state
    let atom = make_tool_call(&app, &sid, "get_atom", json!({
        "agent_id": aid1, "api_token": tok1,
        "atom_id": atom1_id
    }), json!(8)).await.unwrap();
    assert_eq!(atom["result"]["atom_id"].as_str().unwrap(), atom1_id);
    assert_eq!(atom["result"]["lifecycle"].as_str().unwrap(), "provisional");
}

#[serial]
#[tokio::test]
async fn test_cross_feature_interactions() {
    let app = setup_test_app().await;
    let sid = initialize_session(&app).await;
    let (aid, tok) = register(&app, &sid, "cross-test-agent").await;

    // Publish multiple atoms across two domains
    let mut atom_ids = vec![];
    for (i, domain) in [("phys_domain", 3), ("chem_domain", 2)] {
        for j in 0..domain {
            let r = make_tool_call(&app, &sid, "publish", json!({
                "agent_id": aid, "api_token": tok,
                "atom_type": "hypothesis",
                "domain": i,
                "statement": format!("Cross-feature test atom {j} in domain {i}"),
                "conditions": {},
                "provenance": {}
            }), json!(j + 10)).await.unwrap();
            atom_ids.push(r["result"]["atom_id"].as_str().unwrap_or("").to_string());
        }
    }

    // Survey each domain — should get domain-specific results
    for domain in &["phys_domain", "chem_domain"] {
        let survey = make_tool_call(&app, &sid, "survey", json!({
            "agent_id": aid, "api_token": tok,
            "domain": domain,
            "focus": "explore",
            "temperature": 1.0
        }), json!(99)).await.unwrap();
        let suggestions = survey["result"]["suggestions"].as_array().unwrap();
        assert!(!suggestions.is_empty(), "no suggestions for domain {domain}");
    }

    // Retract one atom — should not appear in future survey
    let target_id = &atom_ids[0];
    let retract = make_tool_call(&app, &sid, "retract", json!({
        "agent_id": aid, "api_token": tok,
        "atom_id": target_id,
        "reason": "test retraction"
    }), json!(20)).await.unwrap();
    assert!(retract.get("error").map_or(true, |e| e.is_null()) || retract["result"].is_object(),
        "retract failed: {retract}");
}

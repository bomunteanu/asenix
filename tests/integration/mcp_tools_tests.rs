//! Integration tests for the new 8-tool MCP API.

use axum::http::{Request, Method, StatusCode};
use axum::body::Body;
use serde_json::json;
use super::setup_test_app;
use tower::ServiceExt;
use serial_test::serial;

// ── helpers ──────────────────────────────────────────────────────────────────

async fn init_session(app: &axum::Router) -> String {
    let body = json!({
        "jsonrpc": "2.0", "id": "init-1", "method": "initialize",
        "params": {
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0.0"}
        }
    });
    let req = Request::builder()
        .method(Method::POST).uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let sid = resp.headers().get("mcp-session-id").unwrap().to_str().unwrap().to_string();

    // send initialized notification
    let notif = json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}});
    let req = Request::builder()
        .method(Method::POST).uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &sid)
        .header("content-type", "application/json")
        .body(Body::from(notif.to_string())).unwrap();
    app.clone().oneshot(req).await.unwrap();
    sid
}

async fn tool_call(app: &axum::Router, sid: &str, tool: &str, args: serde_json::Value) -> serde_json::Value {
    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": tool, "arguments": args}
    });
    let req = Request::builder()
        .method(Method::POST).uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", sid)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    // Unwrap content[0].text → parse as JSON
    let text = v["result"]["content"][0]["text"].as_str().unwrap_or("{}");
    serde_json::from_str(text).unwrap_or_else(|_| json!({"raw": text}))
}

// ── tests ────────────────────────────────────────────────────────────────────

#[serial]
#[tokio::test]
async fn test_tools_list_returns_8_tools() {
    let app = setup_test_app().await;
    let sid = init_session(&app).await;

    let body = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}});
    let req = Request::builder()
        .method(Method::POST).uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &sid)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();

    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in &["register", "survey", "get_atom", "publish", "claim", "release_claim", "get_lineage", "retract"] {
        assert!(names.contains(expected), "missing tool: {expected}; got: {names:?}");
    }
    assert_eq!(tools.len(), 8, "expected exactly 8 tools; got: {names:?}");
}

#[serial]
#[tokio::test]
async fn test_register_tool() {
    let app = setup_test_app().await;
    let sid = init_session(&app).await;

    let result = tool_call(&app, &sid, "register", json!({"agent_name": "test-reg-agent"})).await;
    assert!(result["agent_id"].is_string(), "missing agent_id: {result}");
    assert!(result["api_token"].is_string(), "missing api_token: {result}");
    let agent_id = result["agent_id"].as_str().unwrap();
    assert!(!agent_id.is_empty());
}

#[serial]
#[tokio::test]
async fn test_publish_and_get_atom() {
    let app = setup_test_app().await;
    let sid = init_session(&app).await;

    let reg = tool_call(&app, &sid, "register", json!({"agent_name": "pub-agent"})).await;
    let agent_id = reg["agent_id"].as_str().unwrap();
    let api_token = reg["api_token"].as_str().unwrap();

    let pub_result = tool_call(&app, &sid, "publish", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "atom_type": "hypothesis",
        "domain": "test_domain",
        "statement": "Test hypothesis for tool API validation",
        "conditions": {},
        "provenance": {}
    })).await;

    assert!(pub_result["atom_id"].is_string(), "missing atom_id: {pub_result}");
    let atom_id = pub_result["atom_id"].as_str().unwrap();

    // get_atom
    let atom = tool_call(&app, &sid, "get_atom", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "atom_id": atom_id
    })).await;
    assert_eq!(atom["atom_id"].as_str().unwrap(), atom_id);
    assert_eq!(atom["domain"].as_str().unwrap(), "test_domain");
}

#[serial]
#[tokio::test]
async fn test_survey_returns_suggestions() {
    let app = setup_test_app().await;
    let sid = init_session(&app).await;

    let reg = tool_call(&app, &sid, "register", json!({"agent_name": "survey-agent"})).await;
    let agent_id = reg["agent_id"].as_str().unwrap();
    let api_token = reg["api_token"].as_str().unwrap();

    // Publish a few atoms first
    for i in 0..3 {
        tool_call(&app, &sid, "publish", json!({
            "agent_id": agent_id,
            "api_token": api_token,
            "atom_type": "hypothesis",
            "domain": "survey_test",
            "statement": format!("Survey test hypothesis number {i}"),
            "conditions": {},
            "provenance": {}
        })).await;
    }

    let survey = tool_call(&app, &sid, "survey", json!({
        "agent_id": agent_id,
        "api_token": api_token,
        "domain": "survey_test",
        "focus": "explore",
        "temperature": 1.0
    })).await;

    let suggestions = survey["suggestions"].as_array().unwrap();
    assert!(!suggestions.is_empty(), "survey returned no suggestions");
    let first = &suggestions[0];
    assert!(first["atom_id"].is_string(), "suggestion missing atom_id");
}

#[serial]
#[tokio::test]
async fn test_unknown_tool_returns_error() {
    let app = setup_test_app().await;
    let sid = init_session(&app).await;

    let body = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": "register_agent_simple", "arguments": {}}
    });
    let req = Request::builder()
        .method(Method::POST).uri("/mcp")
        .header("origin", "http://localhost:3000")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-session-id", &sid)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["result"]["isError"].as_bool(), Some(true), "expected error for unknown tool: {v}");
}

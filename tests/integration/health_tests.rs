use super::{setup_test_app, make_http_request};

#[tokio::test]
async fn test_health_endpoint_happy_path() {
    let app = setup_test_app().await;
    
    let (status, body) = make_http_request(&app, axum::http::Method::GET, "/health", None)
        .await
        .expect("Failed to make health request");
    
    // Verify 200 OK
    assert_eq!(status, axum::http::StatusCode::OK);
    
    // Parse response as JSON
    let response: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse health response as JSON");
    
    // Verify response structure
    assert!(response["status"].is_string());
    assert_eq!(response["status"].as_str().unwrap(), "healthy");
    
    // Verify database is connected
    assert!(response["database"].is_string());
    assert_eq!(response["database"].as_str().unwrap(), "connected");
    
    // Verify graph cache counts are zero (empty database)
    assert!(response["graph_nodes"].is_number());
    assert_eq!(response["graph_nodes"].as_u64().unwrap(), 0);
    assert_eq!(response["graph_edges"].as_u64().unwrap(), 0);
    assert_eq!(response["embedding_queue_depth"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn test_metrics_endpoint_basic() {
    let app = setup_test_app().await;
    
    let (status, body) = make_http_request(&app, axum::http::Method::GET, "/metrics", None)
        .await
        .expect("Failed to make metrics request");
    
    // Verify 200 OK
    assert_eq!(status, axum::http::StatusCode::OK);
    
    // Verify metrics endpoint returns something (prometheus format)
    assert!(!body.is_empty());
    assert!(body.contains("# HELP"));
    assert!(body.contains("# TYPE"));
}

#[tokio::test]
async fn test_health_endpoint_after_agent_registration() {
    let app = setup_test_app().await;
    
    // Generate a fresh Ed25519 keypair
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    let register_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "register_agent",
        "params": {
            "public_key": public_key_hex
        },
        "id": 1
    });
    
    let (_, _) = make_http_request(&app, axum::http::Method::POST, "/mcp", Some(&register_request.to_string()))
        .await
        .expect("Failed to register agent");
    
    // Now check health endpoint
    let (status, body) = make_http_request(&app, axum::http::Method::GET, "/health", None)
        .await
        .expect("Failed to make health request");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let response: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse health response as JSON");
    
    // Database should still be connected
    assert_eq!(response["database"].as_str().unwrap(), "connected");
    
    // Graph cache should still be empty (agents don't count towards graph cache)
    assert_eq!(response["graph_nodes"].as_u64().unwrap(), 0);
    assert_eq!(response["graph_edges"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn test_metrics_endpoint_after_agent_registration() {
    let app = setup_test_app().await;
    
    // Register an agent
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]); // Use different bytes to avoid collision
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    let register_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "register_agent",
        "params": {
            "public_key": public_key_hex
        },
        "id": 1
    });
    
    let (_, _) = make_http_request(&app, axum::http::Method::POST, "/mcp", Some(&register_request.to_string()))
        .await
        .expect("Failed to register agent");
    
    // Check metrics endpoint
    let (status, body) = make_http_request(&app, axum::http::Method::GET, "/metrics", None)
        .await
        .expect("Failed to make metrics request");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(!body.is_empty());
}

#[tokio::test]
async fn test_health_endpoint_response_structure() {
    let app = setup_test_app().await;
    
    let (status, body) = make_http_request(&app, axum::http::Method::GET, "/health", None)
        .await
        .expect("Failed to make health request");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    let response: serde_json::Value = serde_json::from_str(&body)
        .expect("Failed to parse health response as JSON");
    
    // Verify required fields exist
    assert!(response["status"].is_string());
    assert!(response["database"].is_string());
    assert!(response["graph_nodes"].is_number());
    assert!(response["graph_edges"].is_number());
    assert!(response["embedding_queue_depth"].is_number());
}

#[tokio::test]
async fn test_metrics_endpoint_prometheus_format() {
    let app = setup_test_app().await;
    
    let (status, body) = make_http_request(&app, axum::http::Method::GET, "/metrics", None)
        .await
        .expect("Failed to make metrics request");
    
    assert_eq!(status, axum::http::StatusCode::OK);
    
    // Verify it's in Prometheus format
    let lines: Vec<&str> = body.lines().collect();
    
    // Should have HELP and TYPE comments
    let has_help = lines.iter().any(|line| line.starts_with("# HELP"));
    let has_type = lines.iter().any(|line| line.starts_with("# TYPE"));
    
    assert!(has_help, "Metrics should contain HELP comments");
    assert!(has_type, "Metrics should contain TYPE comments");
    
    // Should have actual metric lines
    let metric_lines: Vec<&str> = lines.iter()
        .map(|&line| line)
        .filter(|line| !line.starts_with("#") && !line.trim().is_empty())
        .collect();
    
    assert!(!metric_lines.is_empty(), "Should have actual metric lines");
}

#[tokio::test]
async fn test_invalid_endpoint_returns_404() {
    let app = setup_test_app().await;
    
    let (status, _) = make_http_request(&app, axum::http::Method::GET, "/invalid", None)
        .await
        .expect("Failed to make request");
    
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_invalid_method_returns_405() {
    let app = setup_test_app().await;
    
    let (status, _) = make_http_request(&app, axum::http::Method::POST, "/health", None)
        .await
        .expect("Failed to make request");
    
    assert_eq!(status, axum::http::StatusCode::METHOD_NOT_ALLOWED);
}

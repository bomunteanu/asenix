use super::{setup_test_app, make_mcp_request};
use ed25519_dalek::Signer;

#[tokio::test]
async fn test_agent_registration_success() {
    let app = setup_test_app().await;
    
    // Generate a fresh Ed25519 keypair
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[0u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    // Call register_agent
    let response = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": public_key_hex
    })), Some(serde_json::json!(1))).await.unwrap();
    
    // Verify response structure
    assert!(response["result"].is_object());
    let result = &response["result"];
    
    // Should contain agent_id (non-empty string)
    assert!(result["agent_id"].is_string());
    let agent_id = result["agent_id"].as_str().unwrap();
    assert!(!agent_id.is_empty());
    
    // Should contain challenge (hex string, 64 characters for 32 bytes)
    assert!(result["challenge"].is_string());
    let challenge = result["challenge"].as_str().unwrap();
    assert_eq!(challenge.len(), 64); // 32 bytes * 2 hex chars
    assert!(hex::decode(challenge).is_ok()); // Valid hex
    
    // Verify no error (error should be null)
    assert!(response.get("error").is_some() && response["error"].is_null(), "Response should have null error field");
}

#[tokio::test]
async fn test_agent_registration_duplicate_public_key() {
    let app = setup_test_app().await;
    
    // Generate a keypair
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[1u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    // Register agent first time
    let response1 = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": public_key_hex.clone()
    })), Some(serde_json::json!(1))).await.unwrap();
    
    assert!(response1["result"].is_object());
    assert!(response1.get("error").is_some() && response1["error"].is_null());
    
    // Try to register same public key again
    let response2 = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": public_key_hex
    })), Some(serde_json::json!(2))).await.unwrap();
    
    // Should return an error
    assert!(response2["error"].is_object());
    let error = &response2["error"];
    assert!(error["code"].is_number());
    assert!(error["message"].is_string());
    
    // Should be some kind of conflict/duplicate error
    let error_code = error["code"].as_i64().unwrap();
    assert!(error_code < 0, "Error code should be negative for server errors");
}

#[tokio::test]
async fn test_agent_confirmation_success() {
    let app = setup_test_app().await;
    
    // Register an agent
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    let register_response = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": public_key_hex
    })), Some(serde_json::json!(1))).await.unwrap();
    
    let agent_id = register_response["result"]["agent_id"].as_str().unwrap();
    let challenge_hex = register_response["result"]["challenge"].as_str().unwrap();
    let challenge_bytes = hex::decode(challenge_hex).unwrap();
    
    // Sign the challenge with the secret key
    let signature_bytes = signing_key.sign(&challenge_bytes).to_bytes();
    let signature_hex = hex::encode(signature_bytes);
    
    // Confirm the agent
    let confirm_response = make_mcp_request(&app, "confirm_agent", Some(serde_json::json!({
        "agent_id": agent_id,
        "signature": signature_hex
    })), Some(serde_json::json!(2))).await.unwrap();
    
    // Should succeed
    assert!(confirm_response["result"].is_object());
    assert!(confirm_response.get("error").is_some() && confirm_response["error"].is_null());
    
    // Should indicate confirmed status
    let result = &confirm_response["result"];
    assert_eq!(result["status"].as_str().unwrap(), "confirmed");
}

#[tokio::test]
async fn test_agent_confirmation_bad_signature() {
    let app = setup_test_app().await;
    
    // Register an agent
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[2u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    let register_response = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": public_key_hex
    })), Some(serde_json::json!(1))).await.unwrap();
    
    let agent_id = register_response["result"]["agent_id"].as_str().unwrap();
    
    // Create a fake signature (64 bytes of zeros)
    let fake_signature = "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    
    // Try to confirm with fake signature
    let confirm_response = make_mcp_request(&app, "confirm_agent", Some(serde_json::json!({
        "agent_id": agent_id,
        "signature": fake_signature
    })), Some(serde_json::json!(2))).await.unwrap();
    
    // Should return authentication error
    assert!(confirm_response["error"].is_object());
    let error = &confirm_response["error"];
    assert!(error["code"].is_number());
    assert!(error["message"].is_string());
    
    // Should be authentication error
    let error_code = error["code"].as_i64().unwrap();
    assert!(error_code < 0, "Error code should be negative for server errors");
}

#[tokio::test]
async fn test_agent_confirmation_invalid_agent_id() {
    let app = setup_test_app().await;
    
    // Try to confirm a non-existent agent
    let fake_signature = "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";
    
    let confirm_response = make_mcp_request(&app, "confirm_agent", Some(serde_json::json!({
        "agent_id": "non-existent-agent-id",
        "signature": fake_signature
    })), Some(serde_json::json!(1))).await.unwrap();
    
    // Should return an error
    assert!(confirm_response["error"].is_object());
    let error = &confirm_response["error"];
    assert!(error["code"].is_number());
    assert!(error["message"].is_string());
    
    // Should be not found or invalid request error
    let error_code = error["code"].as_i64().unwrap();
    assert!(error_code < 0, "Error code should be negative for server errors");
}

#[tokio::test]
async fn test_agent_registration_invalid_public_key() {
    let app = setup_test_app().await;
    
    // Try to register with invalid public key (not hex)
    let response = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": "invalid-hex-string"
    })), Some(serde_json::json!(1))).await.unwrap();
    
    // Should return an error
    assert!(response["error"].is_object());
    let error = &response["error"];
    assert!(error["code"].is_number());
    assert!(error["message"].is_string());
    
    // Should be invalid request error
    let error_code = error["code"].as_i64().unwrap();
    assert!(error_code < 0, "Error code should be negative for server errors");
}

#[tokio::test]
async fn test_agent_registration_missing_public_key() {
    let app = setup_test_app().await;
    
    // Try to register without public key
    let response = make_mcp_request(&app, "register_agent", Some(serde_json::json!({})), Some(serde_json::json!(1))).await.unwrap();
    
    // Should return an error
    assert!(response["error"].is_object());
    let error = &response["error"];
    assert!(error["code"].is_number());
    assert!(error["message"].is_string());
    
    // Should be invalid request error
    let error_code = error["code"].as_i64().unwrap();
    assert!(error_code < 0, "Error code should be negative for server errors");
}

#[tokio::test]
async fn test_agent_confirmation_missing_fields() {
    let app = setup_test_app().await;
    
    // Try to confirm without agent_id
    let response1 = make_mcp_request(&app, "confirm_agent", Some(serde_json::json!({
        "signature": "00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"
    })), Some(serde_json::json!(1))).await.unwrap();
    
    assert!(response1["error"].is_object());
    
    // Try to confirm without signature
    let response2 = make_mcp_request(&app, "confirm_agent", Some(serde_json::json!({
        "agent_id": "some-agent-id"
    })), Some(serde_json::json!(2))).await.unwrap();
    
    assert!(response2["error"].is_object());
}

#[tokio::test]
async fn test_agent_confirmation_invalid_signature_format() {
    let app = setup_test_app().await;
    
    // Register an agent first
    let signing_key = ed25519_dalek::SigningKey::from_bytes(&[4u8; 32]);
    let public_key_hex = hex::encode(signing_key.verifying_key().as_bytes());
    
    let register_response = make_mcp_request(&app, "register_agent", Some(serde_json::json!({
        "public_key": public_key_hex
    })), Some(serde_json::json!(1))).await.unwrap();
    
    let agent_id = register_response["result"]["agent_id"].as_str().unwrap();
    
    // Try to confirm with invalid signature format (not hex, wrong length)
    let invalid_signature = "invalid-signature-format";
    
    let confirm_response = make_mcp_request(&app, "confirm_agent", Some(serde_json::json!({
        "agent_id": agent_id,
        "signature": invalid_signature
    })), Some(serde_json::json!(2))).await.unwrap();
    
    // Should return an error
    assert!(confirm_response["error"].is_object());
    let error = &confirm_response["error"];
    assert!(error["code"].is_number());
    assert!(error["message"].is_string());
    
    // Should be invalid request error
    let error_code = error["code"].as_i64().unwrap();
    assert!(error_code < 0, "Error code should be negative for server errors");
}

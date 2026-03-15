//! Unit tests for MCP session management

use asenix::api::mcp_session::{SessionStore, ClientInfo, Capabilities, ToolsCapability, ResourcesCapability};
use std::time::Duration;

#[tokio::test]
async fn test_session_lifecycle() {
    let store = SessionStore::new();
    
    // Create session
    let client_info = ClientInfo {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    let capabilities = Capabilities {
        tools: Some(ToolsCapability { list_changed: None }),
        resources: Some(ResourcesCapability { list_changed: None }),
    };
    
    let session_id = "test-session-123".to_string();
    store.create_session(
        session_id.clone(),
        client_info,
        capabilities,
        "2025-03-26".to_string(),
        None,
    );

    // Check session exists
    assert!(store.get_session(&session_id).is_some());
    
    // Update activity
    assert!(store.update_activity(&session_id));
    
    // Mark initialized
    assert!(store.mark_initialized(&session_id));
    assert!(store.get_session(&session_id).unwrap().initialized);
    
    // Remove session
    assert!(store.remove_session(&session_id));
    assert!(store.get_session(&session_id).is_none());
}

#[tokio::test]
async fn test_session_cleanup() {
    let store = SessionStore::new();
    
    // Create an old session
    let old_session_id = "old-session".to_string();
    let client_info = ClientInfo {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };
    
    store.create_session(
        old_session_id.clone(),
        client_info,
        Capabilities::default(),
        "2025-03-26".to_string(),
        None,
    );

    // Create a recent session
    let recent_session_id = "recent-session".to_string();
    let client_info = ClientInfo {
        name: "test-client".to_string(),
        version: "1.0.0".to_string(),
    };

    store.create_session(
        recent_session_id.clone(),
        client_info,
        Capabilities::default(),
        "2025-03-26".to_string(),
        None,
    );

    // Mock time passage (2 hours ago)
    let two_hours_ago = std::time::Instant::now() - Duration::from_secs(7200);
    
    // Manually set old session as expired
    {
        let mut sessions = store.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(&old_session_id) {
            session.last_active_at = two_hours_ago;
        }
    }

    // Cleanup should remove old session but keep recent one
    store.cleanup_expired_sessions();
    
    assert!(store.get_session(&old_session_id).is_none());
    assert!(store.get_session(&recent_session_id).is_some());
}

#[tokio::test]
async fn test_session_validation() {
    let store = SessionStore::new();
    
    // Test non-existent session
    assert!(store.get_session("non-existent").is_none());
    assert!(!store.update_activity("non-existent"));
    assert!(!store.mark_initialized("non-existent"));
    assert!(!store.remove_session("non-existent"));
}

#[tokio::test]
async fn test_multiple_sessions() {
    let store = SessionStore::new();
    
    // Create multiple sessions
    let session_ids = vec!["session1", "session2", "session3"];
    
    for session_id in &session_ids {
        let client_info = ClientInfo {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
        };
        
        store.create_session(
            session_id.to_string(),
            client_info,
            Capabilities::default(),
            "2025-03-26".to_string(),
            None,
        );
    }
    
    // All sessions should exist
    for session_id in &session_ids {
        assert!(store.get_session(session_id).is_some());
    }
    
    // Remove one session
    assert!(store.remove_session("session2"));
    assert!(store.get_session("session2").is_none());
    
    // Other sessions should still exist
    assert!(store.get_session("session1").is_some());
    assert!(store.get_session("session3").is_some());
}

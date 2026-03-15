use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::Deserialize;

/// MCP session information
#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub client_info: ClientInfo,
    pub client_capabilities: Capabilities,
    pub protocol_version: String,
    pub created_at: Instant,
    pub last_active_at: Instant,
    pub initialized: bool,
}

/// Client information from initialize request
#[derive(Debug, Clone, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// Server capabilities
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Capabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
}

/// Tools capability
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsCapability {
    #[serde(default, rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Resources capability  
#[derive(Debug, Clone, Deserialize)]
pub struct ResourcesCapability {
    #[serde(default, rename = "listChanged")]
    pub list_changed: Option<bool>,
}

/// Session store for managing MCP sessions
pub struct SessionStore {
    /// Sessions storage - made public for tests
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    pub session_ttl: Duration,
}

impl SessionStore {
    pub fn new() -> Self {
        Self::with_ttl_seconds(1800) // 30-minute idle TTL
    }

    pub fn with_ttl_seconds(session_ttl_seconds: u64) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            session_ttl: Duration::from_secs(session_ttl_seconds),
        }
    }

    /// Create a new session
    pub fn create_session(
        &self,
        session_id: String,
        client_info: ClientInfo,
        client_capabilities: Capabilities,
        protocol_version: String,
    ) -> Session {
        let session = Session {
            session_id: session_id.clone(),
            client_info,
            client_capabilities,
            protocol_version,
            created_at: Instant::now(),
            last_active_at: Instant::now(),
            initialized: false,
        };

        self.sessions.lock().unwrap().insert(session_id.clone(), session.clone());
        session
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<Session> {
        self.sessions.lock().unwrap().get(session_id).cloned()
    }

    /// Update session activity timestamp
    pub fn update_activity(&self, session_id: &str) -> bool {
        {
            // First check if session exists without holding the lock
            let sessions = self.sessions.lock().unwrap();
            if !sessions.contains_key(session_id) {
                return false;
            }
        } // Drop the lock here
        
        // Now acquire lock again for the update
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(s) = sessions.get_mut(session_id) {
            s.last_active_at = Instant::now();
            true
        } else {
            false
        }
    }

    /// Mark session as initialized
    pub fn mark_initialized(&self, session_id: &str) -> bool {
        {
            // First check if session exists without holding the lock
            let sessions = self.sessions.lock().unwrap();
            if !sessions.contains_key(session_id) {
                return false;
            }
        } // Drop the lock here
        
        // Now acquire lock again for the update
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(s) = sessions.get_mut(session_id) {
            s.initialized = true;
            true
        } else {
            false
        }
    }

    /// Remove a session
    pub fn remove_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.remove(session_id).is_some()
    }

    /// Clean up expired sessions (older than 1 hour)
    pub fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        let now = Instant::now();
        
        // Collect expired session IDs
        let expired_sessions: Vec<String> = sessions
            .iter()
            .filter(|(_, session)| now.duration_since(session.last_active_at) > self.session_ttl)
            .map(|(id, _)| id.clone())
            .collect();

        // Remove expired sessions
        for session_id in expired_sessions {
            sessions.remove(&session_id);
        }
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_lifecycle() {
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

    #[test]
    fn test_session_cleanup() {
        let store = SessionStore::new();
        
        // Create an old session
        let old_session_id = "old-session".to_string();
        let client_info = ClientInfo {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
        };
        
        let capabilities = Capabilities {
            tools: Some(ToolsCapability { list_changed: None }),
            resources: Some(ResourcesCapability { list_changed: None }),
        };
        
        store.create_session(
            old_session_id.clone(),
            client_info,
            capabilities,
            "2025-03-26".to_string(),
        );

        // Create a recent session
        let recent_session_id = "recent-session".to_string();
        let client_info = ClientInfo {
            name: "test-client".to_string(),
            version: "1.0.0".to_string(),
        };
        
        store.create_session(
            recent_session_id.clone(),
            client_info.clone(),
            Capabilities::default(),
            "2025-03-26".to_string(),
        );

        // Mock time passage (2 hours ago)
        let two_hours_ago = Instant::now() - Duration::from_secs(7200);
        
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
}

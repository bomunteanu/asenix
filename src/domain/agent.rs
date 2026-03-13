use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: String,
    pub public_key: Vec<u8>,
    pub confirmed: bool,
    pub challenge: Option<Vec<u8>>,
    pub reliability: Option<f64>, // null means probationary
    pub replication_rate: f64,
    pub retraction_rate: f64,
    pub contradiction_rate: f64,
    pub atoms_published: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistration {
    pub public_key: String, // hex-encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfirmation {
    pub agent_id: String,
    pub signature: String, // hex-encoded signature of challenge
}

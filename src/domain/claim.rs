use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub claim_id: String,
    pub atom_id: String,
    pub agent_id: String,
    pub expires_at: DateTime<Utc>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimInput {
    pub hypothesis: String,
    pub conditions: serde_json::Value,
    pub domain: String,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimResponse {
    pub atom_id: String,
    pub neighbourhood: Vec<crate::domain::atom::Atom>,
    pub active_claims: Vec<Claim>,
    pub pheromone_landscape: serde_json::Value,
}

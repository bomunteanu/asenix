use crate::error::{MoteError, Result};
use blake3::Hasher;
use serde_json;

pub fn compute_atom_id(
    atom_type: &str,
    domain: &str,
    statement: &str,
    conditions: &serde_json::Value,
    provenance: &serde_json::Value,
    timestamp: &chrono::DateTime<chrono::Utc>,
) -> String {
    let mut hasher = Hasher::new();
    
    // Hash all atom content deterministically
    hasher.update(atom_type.as_bytes());
    hasher.update(domain.as_bytes());
    hasher.update(statement.as_bytes());
    
    // Hash JSON representation with canonical ordering
    let conditions_json = serde_json::to_string(conditions).unwrap_or_default();
    hasher.update(conditions_json.as_bytes());
    
    let provenance_json = serde_json::to_string(provenance).unwrap_or_default();
    hasher.update(provenance_json.as_bytes());
    
    // Hash timestamp as RFC3339 string
    hasher.update(timestamp.to_rfc3339().as_bytes());
    
    let hash = hasher.finalize();
    hex::encode(hash.as_bytes())
}

pub fn compute_agent_id(public_key: &[u8]) -> String {
    let mut hasher = Hasher::new();
    hasher.update(public_key);
    let hash = hasher.finalize();
    // Truncate to first 16 bytes for agent ID
    hex::encode(&hash.as_bytes()[..16])
}

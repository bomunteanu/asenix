use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: Option<i64>,
    pub source_id: String,
    pub target_id: String,
    pub edge_type: EdgeType,
    pub repl_type: Option<ReplicationType>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EdgeType {
    DerivedFrom,
    InspiredBy,
    Contradicts,
    Replicates,
    Summarizes,
    Supersedes,
    Retracts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplicationType {
    Exact,
    Conceptual,
    Extension,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInput {
    pub source_id: String, // Can be "SELF:N" to reference Nth atom in batch
    pub target_id: String, // Can be "SELF:N" to reference Nth atom in batch
    pub edge_type: EdgeType,
    pub repl_type: Option<ReplicationType>,
}

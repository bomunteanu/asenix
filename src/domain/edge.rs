use serde::{Deserialize, Serialize};

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

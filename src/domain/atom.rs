use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Atom {
    pub atom_id: String,
    pub atom_type: AtomType,
    pub domain: String,
    pub statement: String,
    pub conditions: serde_json::Value,
    pub metrics: Option<serde_json::Value>,
    pub provenance: serde_json::Value,
    pub author_agent_id: String,
    pub created_at: DateTime<Utc>,
    pub signature: Vec<u8>,
    pub artifact_tree_hash: Option<String>,
    
    // Mutable meta fields
    pub confidence: f64,
    pub ph_attraction: f64,
    pub ph_repulsion: f64,
    pub ph_novelty: f64,
    pub ph_disagreement: f64,
    pub embedding: Option<Vec<f64>>,
    pub embedding_status: EmbeddingStatus,
    pub repl_exact: i32,
    pub repl_conceptual: i32,
    pub repl_extension: i32,
    pub traffic: i32,
    pub lifecycle: Lifecycle,
    pub retracted: bool,
    pub retraction_reason: Option<String>,
    pub ban_flag: bool,
    pub archived: bool,
    pub probationary: bool,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AtomType {
    Hypothesis,
    Finding,
    NegativeResult,
    Delta,
    ExperimentLog,
    Synthesis,
    Bounty,
}

impl std::fmt::Display for AtomType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtomType::Hypothesis => write!(f, "hypothesis"),
            AtomType::Finding => write!(f, "finding"),
            AtomType::NegativeResult => write!(f, "negative_result"),
            AtomType::Delta => write!(f, "delta"),
            AtomType::ExperimentLog => write!(f, "experiment_log"),
            AtomType::Synthesis => write!(f, "synthesis"),
            AtomType::Bounty => write!(f, "bounty"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EmbeddingStatus {
    Pending,
    Ready,
}

impl std::fmt::Display for EmbeddingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmbeddingStatus::Pending => write!(f, "pending"),
            EmbeddingStatus::Ready => write!(f, "ready"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Lifecycle {
    Provisional,
    Replicated,
    Core,
    Contested,
}

impl std::fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lifecycle::Provisional => write!(f, "provisional"),
            Lifecycle::Replicated => write!(f, "replicated"),
            Lifecycle::Core => write!(f, "core"),
            Lifecycle::Contested => write!(f, "contested"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomInput {
    pub atom_type: AtomType,
    pub domain: String,
    pub statement: String,
    pub conditions: serde_json::Value,
    pub metrics: Option<serde_json::Value>,
    pub provenance: serde_json::Value,
    pub signature: Vec<u8>,
    pub artifact_tree_hash: Option<String>, // Keep for backward compatibility
    pub artifact_inline: Option<crate::api::artifact_processor::InlineArtifact>, // New inline support
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub parent_ids: Vec<String>,
    pub code_hash: Option<String>,
    pub environment: Option<String>,
    pub dataset_fingerprint: Option<String>,
    pub experiment_ref: Option<String>,
    pub method_description: Option<String>,
}

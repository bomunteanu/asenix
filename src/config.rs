use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub hub: HubConfig,
    pub pheromone: PheromoneConfig,
    pub trust: TrustConfig,
    pub workers: WorkersConfig,
    pub acceptance: AcceptanceConfig,
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HubConfig {
    pub name: String,
    pub domain: String,
    pub listen_address: String,
    pub embedding_endpoint: String,
    pub embedding_model: String,
    pub embedding_dimension: usize,
    pub structured_vector_reserved_dims: usize,
    pub dims_per_numeric_key: usize,
    pub dims_per_categorical_key: usize,
    pub neighbourhood_radius: f64,
    pub summary_llm_endpoint: Option<String>,
    pub summary_llm_model: Option<String>,
    pub artifact_storage_path: String,
    pub max_artifact_blob_bytes: u64,
    pub max_artifact_storage_per_agent_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PheromoneConfig {
    pub decay_half_life_hours: u64,
    pub attraction_cap: f64,
    pub novelty_radius: f64,
    pub disagreement_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    pub reliability_threshold: f64,
    pub independence_ancestry_depth: usize,
    pub probation_atom_count: usize,
    pub max_atoms_per_hour: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkersConfig {
    pub embedding_pool_size: usize,
    pub decay_interval_minutes: u64,
    pub claim_ttl_hours: u64,
    pub staleness_check_interval_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceConfig {
    pub required_provenance_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub allowed_origins: Vec<String>,
}

impl Config {
    pub fn load_from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.hub.embedding_dimension == 0 {
            anyhow::bail!("embedding_dimension must be > 0");
        }
        if self.hub.structured_vector_reserved_dims == 0 {
            anyhow::bail!("structured_vector_reserved_dims must be > 0");
        }
        if self.hub.dims_per_numeric_key == 0 {
            anyhow::bail!("dims_per_numeric_key must be > 0");
        }
        if self.hub.dims_per_categorical_key == 0 {
            anyhow::bail!("dims_per_categorical_key must be > 0");
        }
        if self.workers.embedding_pool_size > 32 {
            anyhow::bail!("embedding_pool_size must be <= 32");
        }
        if self.trust.reliability_threshold < 0.0 || self.trust.reliability_threshold > 1.0 {
            anyhow::bail!("reliability_threshold must be between 0.0 and 1.0");
        }
        
        // Validate pheromone configuration
        if self.pheromone.decay_half_life_hours == 0 {
            anyhow::bail!("decay_half_life_hours must be > 0");
        }
        if self.pheromone.attraction_cap <= 0.0 {
            anyhow::bail!("attraction_cap must be > 0");
        }
        if self.pheromone.novelty_radius <= 0.0 || self.pheromone.novelty_radius > 1.0 {
            anyhow::bail!("novelty_radius must be between 0.0 and 1.0");
        }
        if self.pheromone.disagreement_threshold < 0.0 || self.pheromone.disagreement_threshold > 1.0 {
            anyhow::bail!("disagreement_threshold must be between 0.0 and 1.0");
        }
        
        Ok(())
    }

    pub fn total_embedding_dimension(&self) -> usize {
        self.hub.embedding_dimension + self.hub.structured_vector_reserved_dims
    }
}

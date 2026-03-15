pub mod agent_queries;
pub mod atom_queries;
pub mod project_queries;
pub mod vector_queries;
pub mod pheromone_queries;
pub mod claim_queries;
pub mod review_queries;
pub mod conflict_queries;
pub mod graph_queries;

// Re-export for backward compatibility
pub use agent_queries::*;
pub use atom_queries::*;
pub use project_queries::*;
pub use vector_queries::*;
pub use pheromone_queries::*;
pub use claim_queries::*;
pub use review_queries::*;
pub use conflict_queries::*;
pub use graph_queries::*;

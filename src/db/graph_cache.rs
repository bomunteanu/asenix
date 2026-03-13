use crate::error::{MoteError, Result};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::sync::Arc;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq)]
pub enum EdgeType {
    DerivedFrom,
    InspiredBy,
    Contradicts,
    Replicates,
    Summarizes,
    Supersedes,
    Retracts,
}

impl EdgeType {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "derived_from" => Ok(EdgeType::DerivedFrom),
            "inspired_by" => Ok(EdgeType::InspiredBy),
            "contradicts" => Ok(EdgeType::Contradicts),
            "replicates" => Ok(EdgeType::Replicates),
            "summarizes" => Ok(EdgeType::Summarizes),
            "supersedes" => Ok(EdgeType::Supersedes),
            "retracts" => Ok(EdgeType::Retracts),
            _ => Err(MoteError::Validation(format!("Unknown edge type: {}", s))),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeType::DerivedFrom => "derived_from",
            EdgeType::InspiredBy => "inspired_by",
            EdgeType::Contradicts => "contradicts",
            EdgeType::Replicates => "replicates",
            EdgeType::Summarizes => "summarizes",
            EdgeType::Supersedes => "supersedes",
            EdgeType::Retracts => "retracts",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GraphCache {
    pub graph: DiGraph<String, EdgeType>,
    node_indices: HashMap<String, NodeIndex>,
}

impl GraphCache {
    pub fn new() -> Self {
        Self {
            node_indices: HashMap::new(),
            graph: DiGraph::new(),
        }
    }

    pub async fn load_from_database(pool: &sqlx::PgPool) -> Result<Self> {
        let mut cache = Self::new();
        
        // Load all atoms
        let atom_rows = sqlx::query("SELECT atom_id FROM atoms WHERE NOT retracted AND NOT archived")
            .fetch_all(pool)
            .await?;
        
        for row in atom_rows {
            let atom_id: String = row.get("atom_id");
            cache.add_node(atom_id);
        }
        
        // Load all edges
        let edge_rows = sqlx::query("SELECT source_id, target_id, type FROM edges")
            .fetch_all(pool)
            .await?;
        
        for row in edge_rows {
            let source_id: String = row.get("source_id");
            let target_id: String = row.get("target_id");
            let edge_type_str: String = row.get("type");
            
            if let Ok(edge_type) = EdgeType::from_str(&edge_type_str) {
                if let Err(_) = cache.add_edge(&source_id, &target_id, edge_type) {
                    // Edge already exists, skip
                    continue;
                }
            }
        }
        
        Ok(cache)
    }

    pub fn add_node(&mut self, atom_id: String) -> NodeIndex {
        if let Some(idx) = self.node_indices.get(&atom_id) {
            return *idx;
        }
        let idx = self.graph.add_node(atom_id.clone());
        self.node_indices.insert(atom_id, idx);
        idx
    }

    pub fn add_edge(&mut self, source_id: &str, target_id: &str, edge_type: EdgeType) -> Result<()> {
        let source_idx = self.add_node(source_id.to_string());
        let target_idx = self.add_node(target_id.to_string());
        
        // Check if edge already exists
        if self.graph.find_edge(source_idx, target_idx).is_some() {
            return Err(MoteError::Conflict("Edge already exists".to_string()));
        }

        self.graph.add_edge(source_idx, target_idx, edge_type);
        Ok(())
    }

    pub fn traverse_bfs(
        &self,
        start_atom_id: &str,
        edge_types: &[EdgeType],
        max_depth: usize,
        direction: TraversalDirection,
    ) -> Vec<String> {
        let start_idx = match self.node_indices.get(start_atom_id) {
            Some(idx) => *idx,
            None => return Vec::new(),
        };

        let mut visited = std::collections::HashSet::new();
        let mut result = Vec::new();
        let mut queue = std::collections::VecDeque::new();
        
        queue.push_back((start_idx, 0));
        visited.insert(start_idx);

        while let Some((current_idx, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // Collect neighbors based on direction
            let neighbors: Vec<NodeIndex> = match direction {
                TraversalDirection::Outgoing => {
                    self.graph.neighbors_directed(current_idx, petgraph::Direction::Outgoing).collect()
                },
                TraversalDirection::Incoming => {
                    self.graph.neighbors_directed(current_idx, petgraph::Direction::Incoming).collect()
                },
                TraversalDirection::Both => {
                    self.graph.neighbors_directed(current_idx, petgraph::Direction::Outgoing)
                        .chain(self.graph.neighbors_directed(current_idx, petgraph::Direction::Incoming))
                        .collect()
                }
            };

            for neighbor_idx in neighbors {
                if visited.contains(&neighbor_idx) {
                    continue;
                }

                // Check if any connecting edge matches the requested types
                let edge_matches = match direction {
                    TraversalDirection::Outgoing => {
                        self.graph.find_edge(current_idx, neighbor_idx)
                            .map(|edge_idx| {
                                let edge_weight = self.graph.edge_weight(edge_idx).unwrap();
                                edge_types.contains(edge_weight)
                            })
                            .unwrap_or(false)
                    },
                    TraversalDirection::Incoming => {
                        self.graph.find_edge(neighbor_idx, current_idx)
                            .map(|edge_idx| {
                                let edge_weight = self.graph.edge_weight(edge_idx).unwrap();
                                edge_types.contains(edge_weight)
                            })
                            .unwrap_or(false)
                    },
                    TraversalDirection::Both => {
                        self.graph.find_edge(current_idx, neighbor_idx)
                            .map(|edge_idx| {
                                let edge_weight = self.graph.edge_weight(edge_idx).unwrap();
                                edge_types.contains(edge_weight)
                            })
                            .unwrap_or(false) ||
                        self.graph.find_edge(neighbor_idx, current_idx)
                            .map(|edge_idx| {
                                let edge_weight = self.graph.edge_weight(edge_idx).unwrap();
                                edge_types.contains(edge_weight)
                            })
                            .unwrap_or(false)
                    }
                };

                if edge_matches {
                    if let Some(atom_id) = self.graph.node_weight(neighbor_idx) {
                        result.push(atom_id.clone());
                        visited.insert(neighbor_idx);
                        queue.push_back((neighbor_idx, depth + 1));
                    }
                }
            }
        }

        result
    }

    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

#[derive(Debug, Clone)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

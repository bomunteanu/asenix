use crate::error::{MoteError, Result};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef as _;
use std::collections::HashMap;
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
    cluster_cache: HashMap<String, serde_json::Value>, // Cache for cluster query results
}

impl Default for GraphCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCache {
    pub fn new() -> Self {
        Self {
            node_indices: HashMap::new(),
            graph: DiGraph::new(),
            cluster_cache: HashMap::new(),
        }
    }

    pub async fn load_from_database(pool: &sqlx::PgPool) -> Result<Self> {
        let mut cache = Self {
            node_indices: HashMap::new(),
            graph: DiGraph::new(),
            cluster_cache: HashMap::new(),
        };
        
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

    // Cluster result caching methods
    pub fn get_cluster_result(&self, cache_key: &str) -> Option<serde_json::Value> {
        self.cluster_cache.get(cache_key).cloned()
    }

    pub fn set_cluster_result(&mut self, cache_key: String, result: serde_json::Value) {
        // Simple cache eviction: keep only last 100 entries
        if self.cluster_cache.len() >= 100 {
            // Remove oldest entries (simple FIFO)
            let keys_to_remove: Vec<String> = self.cluster_cache.keys()
                .take(10)
                .cloned()
                .collect();
            for key in keys_to_remove {
                self.cluster_cache.remove(&key);
            }
        }
        
        self.cluster_cache.insert(cache_key, result);
    }

    pub fn clear_cluster_cache(&mut self) {
        self.cluster_cache.clear();
    }

    /// Return all atoms reachable from any atom in `start_ids` within `max_hops`,
    /// along with the edge types encountered.  Optionally filtered to specific
    /// edge type strings.
    pub fn get_subgraph(
        &self,
        start_ids: &[String],
        max_hops: u32,
        edge_type_filter: &Option<Vec<String>>,
    ) -> GraphSubgraph {
        let mut connected_atoms: Vec<String> = Vec::new();
        let mut edge_types_found: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut paths: Vec<Vec<String>> = Vec::new();
        let mut visited: std::collections::HashSet<petgraph::graph::NodeIndex> =
            std::collections::HashSet::new();

        for start_id in start_ids {
            let start_idx = match self.node_indices.get(start_id) {
                Some(idx) => *idx,
                None => continue,
            };

            let mut queue: std::collections::VecDeque<(petgraph::graph::NodeIndex, u32, Vec<String>)> =
                std::collections::VecDeque::new();
            queue.push_back((start_idx, 0, vec![start_id.clone()]));

            while let Some((current_idx, depth, path)) = queue.pop_front() {
                if depth >= max_hops {
                    continue;
                }

                for neighbor_idx in self.graph.neighbors_directed(current_idx, petgraph::Direction::Outgoing) {
                    if visited.contains(&neighbor_idx) {
                        continue;
                    }

                    let edge_idx = match self.graph.find_edge(current_idx, neighbor_idx) {
                        Some(e) => e,
                        None => continue,
                    };
                    let edge_type = self.graph.edge_weight(edge_idx).unwrap();
                    let edge_type_str = edge_type.as_str().to_string();

                    // Apply optional filter
                    if let Some(filter) = edge_type_filter {
                        if !filter.contains(&edge_type_str) {
                            continue;
                        }
                    }

                    edge_types_found.insert(edge_type_str);

                    if let Some(atom_id) = self.graph.node_weight(neighbor_idx) {
                        let mut new_path = path.clone();
                        new_path.push(atom_id.clone());

                        if !connected_atoms.contains(atom_id) {
                            connected_atoms.push(atom_id.clone());
                        }
                        paths.push(new_path.clone());
                        visited.insert(neighbor_idx);
                        queue.push_back((neighbor_idx, depth + 1, new_path));
                    }
                }
            }
        }

        GraphSubgraph {
            hops_explored: max_hops,
            connected_atoms,
            edge_types_found: edge_types_found.into_iter().collect(),
            paths,
        }
    }

    /// BFS/DFS traversal for `get_lineage`. Returns all nodes and edges reachable
    /// from `start_atom_id` within `max_depth` hops.
    /// `direction`: "ancestors" (incoming), "descendants" (outgoing), or "both".
    /// `edge_type_filter`: optional whitelist of edge type strings.
    pub fn traverse(
        &self,
        start_atom_id: &str,
        direction: &str,
        max_depth: usize,
        edge_type_filter: Option<&[String]>,
    ) -> LineageSubgraph {
        use std::collections::{HashSet, VecDeque};

        let start_idx = match self.node_indices.get(start_atom_id) {
            Some(idx) => *idx,
            None => {
                return LineageSubgraph {
                    nodes: vec![start_atom_id.to_string()],
                    edges: vec![],
                };
            }
        };

        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut nodes: Vec<String> = vec![start_atom_id.to_string()];
        let mut edges: Vec<LineageEdge> = Vec::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();

        visited.insert(start_idx);
        queue.push_back((start_idx, 0));

        while let Some((current_idx, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // Collect candidate (neighbor, edge) pairs based on direction
            let mut candidates: Vec<(NodeIndex, NodeIndex)> = Vec::new(); // (from, to)

            match direction {
                "ancestors" => {
                    for neighbor in self.graph.neighbors_directed(current_idx, petgraph::Direction::Incoming) {
                        candidates.push((neighbor, current_idx));
                    }
                }
                "descendants" => {
                    for neighbor in self.graph.neighbors_directed(current_idx, petgraph::Direction::Outgoing) {
                        candidates.push((current_idx, neighbor));
                    }
                }
                _ /* "both" */ => {
                    for neighbor in self.graph.neighbors_directed(current_idx, petgraph::Direction::Outgoing) {
                        candidates.push((current_idx, neighbor));
                    }
                    for neighbor in self.graph.neighbors_directed(current_idx, petgraph::Direction::Incoming) {
                        candidates.push((neighbor, current_idx));
                    }
                }
            }

            for (src_idx, tgt_idx) in candidates {
                let neighbor_idx = if src_idx == current_idx { tgt_idx } else { src_idx };

                let edge_idx = match self.graph.find_edge(src_idx, tgt_idx) {
                    Some(e) => e,
                    None => continue,
                };
                let edge_type_str = self.graph.edge_weight(edge_idx).unwrap().as_str().to_string();

                // Apply optional edge type filter
                if let Some(filter) = edge_type_filter {
                    if !filter.contains(&edge_type_str) {
                        continue;
                    }
                }

                let src_id = self.graph.node_weight(src_idx).cloned().unwrap_or_default();
                let tgt_id = self.graph.node_weight(tgt_idx).cloned().unwrap_or_default();

                edges.push(LineageEdge {
                    source: src_id,
                    target: tgt_id,
                    edge_type: edge_type_str,
                });

                if !visited.contains(&neighbor_idx) {
                    visited.insert(neighbor_idx);
                    if let Some(atom_id) = self.graph.node_weight(neighbor_idx) {
                        nodes.push(atom_id.clone());
                    }
                    queue.push_back((neighbor_idx, depth + 1));
                }
            }
        }

        LineageSubgraph { nodes, edges }
    }

    /// Return all outgoing edge triples (source, target, edge_type) for a given atom.
    pub fn get_edges(&self, atom_id: &str) -> Vec<serde_json::Value> {
        let idx = match self.node_indices.get(atom_id) {
            Some(i) => *i,
            None => return vec![],
        };
        self.graph
            .edges_directed(idx, petgraph::Direction::Outgoing)
            .filter_map(|edge_ref| {
                let tgt = self.graph.node_weight(edge_ref.target())?;
                Some(serde_json::json!({
                    "source": atom_id,
                    "target": tgt,
                    "edge_type": edge_ref.weight().as_str()
                }))
            })
            .chain(
                self.graph
                    .edges_directed(idx, petgraph::Direction::Incoming)
                    .filter_map(|edge_ref| {
                        let src = self.graph.node_weight(edge_ref.source())?;
                        Some(serde_json::json!({
                            "source": src,
                            "target": atom_id,
                            "edge_type": edge_ref.weight().as_str()
                        }))
                    })
            )
            .collect()
    }
}

pub struct GraphSubgraph {
    pub hops_explored: u32,
    pub connected_atoms: Vec<String>,
    pub edge_types_found: Vec<String>,
    pub paths: Vec<Vec<String>>,
}

/// A lightweight subgraph returned by [`GraphCache::traverse`].
pub struct LineageSubgraph {
    /// All node atom_ids reachable (including the start node).
    pub nodes: Vec<String>,
    /// All edges in the subgraph.
    pub edges: Vec<LineageEdge>,
}

pub struct LineageEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String,
}

#[derive(Debug, Clone)]
pub enum TraversalDirection {
    Outgoing,
    Incoming,
    Both,
}

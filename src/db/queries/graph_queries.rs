use crate::error::Result;
use sqlx::{PgPool, Row};

#[derive(Debug, Default)]
pub struct GraphTraversalInfo {
    pub hops_explored: u32,
    pub connected_atoms: Vec<String>,
    pub edge_types_found: Vec<String>,
    pub paths: Vec<Vec<String>>, // Each path is a sequence of atom_ids
}

/// Get graph traversal information for a set of atoms
pub async fn get_graph_traversal_info(
    pool: &PgPool,
    atom_ids: &[String],
    max_hops: u32,
    edge_types_filter: Option<&[String]>,
) -> Result<GraphTraversalInfo> {
    let mut connected_atoms = Vec::new();
    let mut edge_types_found = std::collections::HashSet::new();
    let mut paths = Vec::new();
    
    // Build edge type filter clause
    let edge_type_clause = if let Some(filter) = edge_types_filter {
        let placeholders: Vec<String> = filter.iter().map(|_| "?".to_string()).collect();
        format!("AND e.type IN ({})", placeholders.join(","))
    } else {
        String::new()
    };
    
    // Find connected atoms within max_hops
    for atom_id in atom_ids {
        let query = format!(
            "WITH RECURSIVE connected_atoms(atom_id, hop, path) AS (
                SELECT target_id, 1, ARRAY[target_id] 
                FROM edges 
                WHERE source_id = $1 {}
                UNION ALL
                SELECT e.target_id, ca.hop + 1, ca.path || e.target_id
                FROM edges e
                JOIN connected_atoms ca ON e.source_id = ca.atom_id
                WHERE ca.hop < {}
                AND NOT e.target_id = ANY(ca.path)
                {}
            )
            SELECT DISTINCT atom_id, hop, path
            FROM connected_atoms",
            edge_type_clause, max_hops, edge_type_clause
        );
        
        let mut query_builder = sqlx::query(&query).bind(atom_id);
        
        // Add edge type filter values if provided
        if let Some(filter) = edge_types_filter {
            for edge_type in filter {
                query_builder = query_builder.bind(edge_type);
            }
        }
        
        let rows = query_builder
            .fetch_all(pool)
            .await?;
        
        for row in rows {
            let connected_id: String = row.get("atom_id");
            let path: Vec<String> = row.get("path");
            
            if !connected_atoms.contains(&connected_id) {
                connected_atoms.push(connected_id);
            }
            
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    
    // Get edge types found
    let edge_query = if let Some(filter) = edge_types_filter {
        let placeholders: Vec<String> = filter.iter().map(|_| "?".to_string()).collect();
        format!(
            "SELECT DISTINCT type FROM edges 
             WHERE (source_id = ANY($1) OR target_id = ANY($1))
             AND type IN ({})",
            placeholders.join(",")
        )
    } else {
        "SELECT DISTINCT type FROM edges WHERE (source_id = ANY($1) OR target_id = ANY($1))".to_string()
    };
    
    let mut edge_query_builder = sqlx::query(&edge_query).bind(&connected_atoms);
    
    if let Some(filter) = edge_types_filter {
        for edge_type in filter {
            edge_query_builder = edge_query_builder.bind(edge_type);
        }
    }
    
    let edge_rows = edge_query_builder
        .fetch_all(pool)
        .await?;
    
    for row in edge_rows {
        let edge_type: String = row.get("type");
        edge_types_found.insert(edge_type);
    }
    
    Ok(GraphTraversalInfo {
        hops_explored: max_hops,
        connected_atoms,
        edge_types_found: edge_types_found.into_iter().collect(),
        paths,
    })
}

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Atom {
    pub atom_id: String,
    pub atom_type: String,
    pub domain: String,
    pub statement: String,
    pub conditions: serde_json::Value,
    pub metrics: Option<serde_json::Value>,
    pub lifecycle: String,
    pub ph_attraction: f64,
    pub ph_repulsion: f64,
    pub ph_novelty: f64,
    pub ph_disagreement: f64,
    pub ban_flag: bool,
    pub retracted: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchAtomsResponse {
    pub atoms: Vec<Atom>,
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub timestamp: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Edge {
    pub source_id: String,
    pub target_id: String,
    pub edge_type: String,
    pub repl_type: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphResponse {
    pub atoms: Vec<Atom>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Serialize)]
pub struct GraphWithEmbeddingsResponse {
    pub atoms: Vec<Atom>,
    pub edges: Vec<Edge>,
    /// atom_id → flat f32 embedding vector (only present when embedding_status = 'ready')
    pub embeddings: std::collections::HashMap<String, Vec<f32>>,
}

#[derive(Debug, Deserialize)]
pub struct SearchAtomsInput {
    pub domain: Option<String>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
    pub lifecycle: Option<String>,
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// rspc-style request wrapper
#[derive(Debug, Deserialize)]
pub struct RspcRequest<T> {
    pub method: String,
    pub params: Option<T>,
}

// rspc-style response wrapper
#[derive(Debug, Serialize)]
pub struct RspcResponse<T> {
    pub result: T,
}

pub async fn handle_rspc_request(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RspcRequest<serde_json::Value>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match request.method.as_str() {
        "health" => {
            let response = HealthResponse {
                status: "healthy".to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            };
            Ok(Json(serde_json::to_value(RspcResponse { result: response }).unwrap()))
        }
        "searchAtoms" => {
            // Parse parameters (same structure as /rpc endpoint)
            let params = request.params.unwrap_or(serde_json::json!({}));
            
            let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
            let type_filter: Option<String> = serde_json::from_value(params["type"].clone()).ok();
            let lifecycle_filter: Option<String> = serde_json::from_value(params["lifecycle"].clone()).ok();
            let text_search: Option<String> = serde_json::from_value(params["query"].clone()).ok();
            let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(50);
            let offset: i64 = serde_json::from_value(params["offset"].clone()).unwrap_or(0);

            // Count total matching atoms (ignores limit/offset)
            let total: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM atoms WHERE NOT retracted AND NOT archived"
            )
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);

            // Call real database function (same as /rpc endpoint)
            let atoms = crate::db::queries::search_atoms(
                &state.pool,
                domain_filter.as_deref(),
                type_filter.as_deref(),
                lifecycle_filter.as_deref(),
                text_search.as_deref(),
                limit,
                offset,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            // Convert database results to rspc format
            let atoms: Vec<Atom> = atoms.into_iter().map(|a| Atom {
                atom_id: a.atom_id,
                atom_type: a.atom_type.to_string(),
                domain: a.domain,
                statement: a.statement,
                conditions: a.conditions,
                metrics: a.metrics,
                lifecycle: a.lifecycle.to_string(),
                ph_attraction: a.ph_attraction,
                ph_repulsion: a.ph_repulsion,
                ph_novelty: a.ph_novelty,
                ph_disagreement: a.ph_disagreement,
                ban_flag: a.ban_flag,
                retracted: a.retracted,
                created_at: a.created_at.to_rfc3339(),
            }).collect();

            let response = SearchAtomsResponse { atoms, total };
            Ok(Json(serde_json::to_value(RspcResponse { result: response }).unwrap()))
        }
        "getGraph" => {
            // Get all atoms
            let atoms = crate::db::queries::search_atoms(
                &state.pool,
                None,
                None,
                None,
                None,
                1000, // Get all atoms
                0,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            // Convert database results to rspc format
            let atoms: Vec<Atom> = atoms.into_iter().map(|a| Atom {
                atom_id: a.atom_id,
                atom_type: a.atom_type.to_string(),
                domain: a.domain,
                statement: a.statement,
                conditions: a.conditions,
                metrics: a.metrics,
                lifecycle: a.lifecycle.to_string(),
                ph_attraction: a.ph_attraction,
                ph_repulsion: a.ph_repulsion,
                ph_novelty: a.ph_novelty,
                ph_disagreement: a.ph_disagreement,
                ban_flag: a.ban_flag,
                retracted: a.retracted,
                created_at: a.created_at.to_rfc3339(),
            }).collect();

            // Get edges using existing RPC handler
            let edges_result = crate::api::rpc_handlers::rpc_backup::handle_get_graph_edges(&state).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let edges_data = edges_result.get("edges").and_then(|e| e.as_array())
                .ok_or_else(|| StatusCode::INTERNAL_SERVER_ERROR)?;

            let edges: Vec<Edge> = edges_data.iter().filter_map(|edge| {
                Some(Edge {
                    source_id: edge.get("source_id")?.as_str()?.to_string(),
                    target_id: edge.get("target_id")?.as_str()?.to_string(),
                    edge_type: edge.get("edge_type")?.as_str()?.to_string(),
                    repl_type: edge.get("repl_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    created_at: edge.get("created_at")?.as_str()?.to_string(),
                })
            }).collect();

            let response = GraphResponse { atoms, edges };
            Ok(Json(serde_json::to_value(RspcResponse { result: response }).unwrap()))
        }
        "getGraphWithEmbeddings" => {
            // Fetch atoms (same as getGraph)
            let atoms = crate::db::queries::search_atoms(
                &state.pool, None, None, None, None, 1000, 0,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let atoms: Vec<Atom> = atoms.into_iter().map(|a| Atom {
                atom_id: a.atom_id,
                atom_type: a.atom_type.to_string(),
                domain: a.domain,
                statement: a.statement,
                conditions: a.conditions,
                metrics: a.metrics,
                lifecycle: a.lifecycle.to_string(),
                ph_attraction: a.ph_attraction,
                ph_repulsion: a.ph_repulsion,
                ph_novelty: a.ph_novelty,
                ph_disagreement: a.ph_disagreement,
                ban_flag: a.ban_flag,
                retracted: a.retracted,
                created_at: a.created_at.to_rfc3339(),
            }).collect();

            // Fetch edges
            let edges_result = crate::api::rpc_handlers::rpc_backup::handle_get_graph_edges(&state).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            let edges_data = edges_result.get("edges").and_then(|e| e.as_array())
                .ok_or_else(|| StatusCode::INTERNAL_SERVER_ERROR)?;
            let edges: Vec<Edge> = edges_data.iter().filter_map(|edge| {
                Some(Edge {
                    source_id: edge.get("source_id")?.as_str()?.to_string(),
                    target_id: edge.get("target_id")?.as_str()?.to_string(),
                    edge_type: edge.get("edge_type")?.as_str()?.to_string(),
                    repl_type: edge.get("repl_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    created_at: edge.get("created_at")?.as_str()?.to_string(),
                })
            }).collect();

            // Fetch embeddings for all ready atoms in one query (cast vector → float4[])
            let emb_rows = sqlx::query(
                "SELECT atom_id, embedding::float4[] AS emb \
                 FROM atoms \
                 WHERE embedding_status = 'ready' AND NOT retracted AND NOT archived"
            )
            .fetch_all(&state.pool)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let mut embeddings: std::collections::HashMap<String, Vec<f32>> =
                std::collections::HashMap::new();
            for row in emb_rows {
                use sqlx::Row;
                let atom_id: String = row.get("atom_id");
                let emb: Vec<f32> = row.get("emb");
                embeddings.insert(atom_id, emb);
            }

            let response = GraphWithEmbeddingsResponse { atoms, edges, embeddings };
            Ok(Json(serde_json::to_value(RspcResponse { result: response }).unwrap()))
        }
        "publish_atoms" => {
            let result = crate::api::rpc_handlers::rpc_backup::handle_publish_atoms(
                &state,
                request.params,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(serde_json::to_value(RspcResponse { result }).unwrap()))
        }
        "ban_atom" => {
            let params = request.params.unwrap_or(serde_json::json!({}));
            let atom_id: String = serde_json::from_value(params["atom_id"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            crate::db::queries::ban_atom(&state.pool, &atom_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(serde_json::to_value(RspcResponse { result: serde_json::json!({ "status": "banned" }) }).unwrap()))
        }
        "unban_atom" => {
            let params = request.params.unwrap_or(serde_json::json!({}));
            let atom_id: String = serde_json::from_value(params["atom_id"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            crate::db::queries::unban_atom(&state.pool, &atom_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(serde_json::to_value(RspcResponse { result: serde_json::json!({ "status": "unbanned" }) }).unwrap()))
        }
        _ => Err(StatusCode::NOT_FOUND),
    }
}

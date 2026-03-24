use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Json,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Atom {
    pub atom_id: String,
    pub atom_type: String,
    pub domain: String,
    pub project_id: Option<String>,
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
    pub project_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Project {
    pub project_id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListProjectsResponse {
    pub projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectInput {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
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

fn domain_atom_to_rspc(a: crate::domain::atom::Atom) -> Atom {
    Atom {
        atom_id: a.atom_id,
        atom_type: a.atom_type.to_string(),
        domain: a.domain,
        project_id: a.project_id,
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
    }
}

fn require_owner_jwt(headers: &HeaderMap) -> Result<(), StatusCode> {
    let secret = std::env::var("OWNER_SECRET").unwrap_or_default();
    if secret.is_empty() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if crate::api::auth::verify_owner_jwt(token, &secret) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

pub async fn handle_rspc_request(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
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
            let params = request.params.unwrap_or(serde_json::json!({}));

            let domain_filter: Option<String> = serde_json::from_value(params["domain"].clone()).ok();
            let type_filter: Option<String> = serde_json::from_value(params["type"].clone()).ok();
            let lifecycle_filter: Option<String> = serde_json::from_value(params["lifecycle"].clone()).ok();
            let text_search: Option<String> = serde_json::from_value(params["query"].clone()).ok();
            let project_id_filter: Option<String> = serde_json::from_value(params["project_id"].clone()).ok();
            let limit: i64 = serde_json::from_value(params["limit"].clone()).unwrap_or(50);
            let offset: i64 = serde_json::from_value(params["offset"].clone()).unwrap_or(0);

            // Count total matching atoms (respects project filter)
            let total: i64 = if let Some(ref pid) = project_id_filter {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM atoms WHERE NOT retracted AND NOT archived AND project_id = $1"
                )
                .bind(pid)
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0)
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM atoms WHERE NOT retracted AND NOT archived"
                )
                .fetch_one(&state.pool)
                .await
                .unwrap_or(0)
            };

            let atoms = crate::db::queries::search_atoms(
                &state.pool,
                domain_filter.as_deref(),
                type_filter.as_deref(),
                lifecycle_filter.as_deref(),
                text_search.as_deref(),
                project_id_filter.as_deref(),
                limit,
                offset,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let atoms: Vec<Atom> = atoms.into_iter().map(domain_atom_to_rspc).collect();
            let response = SearchAtomsResponse { atoms, total };
            Ok(Json(serde_json::to_value(RspcResponse { result: response }).unwrap()))
        }
        "getGraph" => {
            let params = request.params.clone().unwrap_or(serde_json::json!({}));
            let project_id_filter: Option<String> = serde_json::from_value(params["project_id"].clone()).ok();

            let atoms = crate::db::queries::search_atoms(
                &state.pool,
                None,
                None,
                None,
                None,
                project_id_filter.as_deref(),
                1000,
                0,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let atoms: Vec<Atom> = atoms.into_iter().map(domain_atom_to_rspc).collect();

            let edge_params = project_id_filter.as_ref().map(|pid| serde_json::json!({ "project_id": pid }));
            let edges_result = crate::api::rpc_handlers::rpc_impl::handle_get_graph_edges(&state, edge_params).await
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
            let params = request.params.clone().unwrap_or(serde_json::json!({}));
            let project_id_filter: Option<String> = serde_json::from_value(params["project_id"].clone()).ok();

            let atoms = crate::db::queries::search_atoms(
                &state.pool,
                None,
                None,
                None,
                None,
                project_id_filter.as_deref(),
                1000,
                0,
            ).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let atom_ids: Vec<String> = atoms.iter().map(|a| a.atom_id.clone()).collect();
            let atoms: Vec<Atom> = atoms.into_iter().map(domain_atom_to_rspc).collect();

            let edge_params = project_id_filter.as_ref().map(|pid| serde_json::json!({ "project_id": pid }));
            let edges_result = crate::api::rpc_handlers::rpc_impl::handle_get_graph_edges(&state, edge_params).await
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

            // Fetch embeddings — scoped to the atom_ids already filtered by project
            let emb_rows = if atom_ids.is_empty() {
                vec![]
            } else {
                sqlx::query(
                    "SELECT atom_id, embedding::float4[] AS emb \
                     FROM atoms \
                     WHERE embedding_status = 'ready' AND NOT retracted AND NOT archived \
                     AND atom_id = ANY($1)"
                )
                .bind(&atom_ids)
                .fetch_all(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            };

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
        "listProjects" => {
            let projects = crate::db::queries::list_projects(&state.pool)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let projects: Vec<Project> = projects.into_iter().map(|p| Project {
                project_id: p.project_id,
                name: p.name,
                slug: p.slug,
                description: p.description,
                created_at: p.created_at.to_rfc3339(),
            }).collect();

            let response = ListProjectsResponse { projects };
            Ok(Json(serde_json::to_value(RspcResponse { result: response }).unwrap()))
        }
        "getProject" => {
            let params = request.params.unwrap_or(serde_json::json!({}));
            let project_id: String = serde_json::from_value(params["project_id"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            let p = crate::db::queries::get_project(&state.pool, &project_id)
                .await
                .map_err(|_| StatusCode::NOT_FOUND)?;

            let project = Project {
                project_id: p.project_id,
                name: p.name,
                slug: p.slug,
                description: p.description,
                created_at: p.created_at.to_rfc3339(),
            };
            Ok(Json(serde_json::to_value(RspcResponse { result: project }).unwrap()))
        }
        "createProject" => {
            let params = request.params.unwrap_or(serde_json::json!({}));
            let name: String = serde_json::from_value(params["name"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let slug: String = serde_json::from_value(params["slug"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let description: Option<String> = serde_json::from_value(params["description"].clone()).ok();

            let p = crate::db::queries::create_project(
                &state.pool,
                &name,
                &slug,
                description.as_deref(),
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let project = Project {
                project_id: p.project_id,
                name: p.name,
                slug: p.slug,
                description: p.description,
                created_at: p.created_at.to_rfc3339(),
            };
            Ok(Json(serde_json::to_value(RspcResponse { result: project }).unwrap()))
        }
        "updateProject" => {
            let params = request.params.unwrap_or(serde_json::json!({}));
            let project_id: String = serde_json::from_value(params["project_id"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let name: String = serde_json::from_value(params["name"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let slug: String = serde_json::from_value(params["slug"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            let description: Option<String> = serde_json::from_value(params["description"].clone()).ok();

            let p = crate::db::queries::update_project(
                &state.pool,
                &project_id,
                &name,
                &slug,
                description.as_deref(),
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            let project = Project {
                project_id: p.project_id,
                name: p.name,
                slug: p.slug,
                description: p.description,
                created_at: p.created_at.to_rfc3339(),
            };
            Ok(Json(serde_json::to_value(RspcResponse { result: project }).unwrap()))
        }
        "deleteProject" => {
            let params = request.params.unwrap_or(serde_json::json!({}));
            let project_id: String = serde_json::from_value(params["project_id"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            crate::db::queries::delete_project(&state.pool, &project_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(serde_json::to_value(RspcResponse { result: serde_json::json!({ "status": "deleted" }) }).unwrap()))
        }
        "publish_atoms" => {
            let result = crate::api::rpc_handlers::rpc_impl::handle_publish_atoms(
                &state,
                request.params,
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(serde_json::to_value(RspcResponse { result }).unwrap()))
        }
        "ban_atom" => {
            require_owner_jwt(&headers)?;
            let params = request.params.unwrap_or(serde_json::json!({}));
            let atom_id: String = serde_json::from_value(params["atom_id"].clone())
                .map_err(|_| StatusCode::BAD_REQUEST)?;

            crate::db::queries::ban_atom(&state.pool, &atom_id)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

            Ok(Json(serde_json::to_value(RspcResponse { result: serde_json::json!({ "status": "banned" }) }).unwrap()))
        }
        "unban_atom" => {
            require_owner_jwt(&headers)?;
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

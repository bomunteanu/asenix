use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(serde::Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub database: String,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub embedding_queue_depth: usize,
}

#[derive(Default)]
pub struct Metrics {
    // Counters
    pub publish_requests_accepted: AtomicU64,
    pub publish_requests_rejected: AtomicU64,
    pub publish_requests_queued: AtomicU64,
    pub rate_limit_rejections: AtomicU64,
    pub embedding_jobs_completed: AtomicU64,
    pub embedding_jobs_failed: AtomicU64,
    pub contradictions_detected: AtomicU64,
}

impl Metrics {
    pub async fn format_prometheus(&self, state: &AppState) -> Result<String, sqlx::Error> {
        let mut output = String::new();
        
        // Counters
        output.push_str("# HELP mote_publish_requests_total Total number of publish requests\n");
        output.push_str("# TYPE mote_publish_requests_total counter\n");
        output.push_str(&format!("mote_publish_requests_total{{status=\"accepted\"}} {}\n", 
            self.publish_requests_accepted.load(Ordering::Relaxed)));
        output.push_str(&format!("mote_publish_requests_total{{status=\"rejected\"}} {}\n", 
            self.publish_requests_rejected.load(Ordering::Relaxed)));
        output.push_str(&format!("mote_publish_requests_total{{status=\"queued\"}} {}\n", 
            self.publish_requests_queued.load(Ordering::Relaxed)));
        
        output.push_str("# HELP mote_rate_limit_rejections_total Total number of rate limit rejections\n");
        output.push_str("# TYPE mote_rate_limit_rejections_total counter\n");
        output.push_str(&format!("mote_rate_limit_rejections_total {}\n", 
            self.rate_limit_rejections.load(Ordering::Relaxed)));
        
        output.push_str("# HELP mote_embedding_jobs_total Total number of embedding jobs\n");
        output.push_str("# TYPE mote_embedding_jobs_total counter\n");
        output.push_str(&format!("mote_embedding_jobs_total{{status=\"completed\"}} {}\n", 
            self.embedding_jobs_completed.load(Ordering::Relaxed)));
        output.push_str(&format!("mote_embedding_jobs_total{{status=\"failed\"}} {}\n", 
            self.embedding_jobs_failed.load(Ordering::Relaxed)));
        
        output.push_str("# HELP mote_contradictions_detected_total Total number of contradictions detected\n");
        output.push_str("# TYPE mote_contradictions_detected_total counter\n");
        output.push_str(&format!("mote_contradictions_detected_total {}\n", 
            self.contradictions_detected.load(Ordering::Relaxed)));
        
        // Database gauges
        let atoms_by_type = sqlx::query(
            "SELECT type, lifecycle, domain, COUNT(*) as count 
             FROM atoms WHERE NOT archived GROUP BY type, lifecycle, domain"
        )
        .fetch_all(&state.pool)
        .await?;
        
        output.push_str("# HELP mote_atoms_total Total number of atoms by type, lifecycle, and domain\n");
        output.push_str("# TYPE mote_atoms_total gauge\n");
        for row in atoms_by_type {
            let atom_type: String = row.get("type");
            let lifecycle: String = row.get("lifecycle");
            let domain: String = row.get("domain");
            let count: i64 = row.get("count");
            output.push_str(&format!("mote_atoms_total{{type=\"{}\",lifecycle=\"{}\",domain=\"{}\"}} {}\n", 
                atom_type, lifecycle, domain, count));
        }
        
        let edges_by_type = sqlx::query("SELECT type, COUNT(*) as count FROM edges GROUP BY type")
            .fetch_all(&state.pool)
            .await?;
        
        output.push_str("# HELP mote_edges_total Total number of edges by type\n");
        output.push_str("# TYPE mote_edges_total gauge\n");
        for row in edges_by_type {
            let edge_type: String = row.get("type");
            let count: i64 = row.get("count");
            output.push_str(&format!("mote_edges_total{{type=\"{}\"}} {}\n", edge_type, count));
        }
        
        let agents_by_confirmed = sqlx::query("SELECT confirmed, COUNT(*) as count FROM agents GROUP BY confirmed")
            .fetch_all(&state.pool)
            .await?;
        
        output.push_str("# HELP mote_agents_total Total number of agents by confirmation status\n");
        output.push_str("# TYPE mote_agents_total gauge\n");
        for row in agents_by_confirmed {
            let confirmed: bool = row.get("confirmed");
            let count: i64 = row.get("count");
            output.push_str(&format!("mote_agents_total{{confirmed=\"{}\"}} {}\n", confirmed, count));
        }
        
        let active_claims: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM claims WHERE active = TRUE")
            .fetch_one(&state.pool)
            .await?;
        
        output.push_str("# HELP mote_claims_active Number of active claims\n");
        output.push_str("# TYPE mote_claims_active gauge\n");
        output.push_str(&format!("mote_claims_active {}\n", active_claims));
        
        // In-memory gauges
        let cache = state.graph_cache.read().await;
        output.push_str("# HELP mote_graph_cache_nodes Number of nodes in graph cache\n");
        output.push_str("# TYPE mote_graph_cache_nodes gauge\n");
        output.push_str(&format!("mote_graph_cache_nodes {}\n", cache.graph.node_count()));
        
        output.push_str("# HELP mote_graph_cache_edges Number of edges in graph cache\n");
        output.push_str("# TYPE mote_graph_cache_edges gauge\n");
        output.push_str(&format!("mote_graph_cache_edges {}\n", cache.graph.edge_count()));
        
        output.push_str("# HELP mote_embedding_queue_depth Current embedding queue depth\n");
        output.push_str("# TYPE mote_embedding_queue_depth gauge\n");
        output.push_str(&format!("mote_embedding_queue_depth {}\n", 0)); // TODO: Implement queue depth tracking
        
        Ok(output)
    }
}

#[derive(Deserialize)]
pub struct ReviewQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub domain: Option<String>,
}

#[derive(Deserialize)]
pub struct ReviewAction {
    pub action: String, // "approve", "reject"
    pub reason: Option<String>,
}

pub async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let cache = state.graph_cache.read().await;
    let graph_nodes = cache.graph.node_count();
    let graph_edges = cache.graph.edge_count();
    
    Json(HealthResponse {
        status: "healthy".to_string(),
        database: "connected".to_string(),
        graph_nodes,
        graph_edges,
        embedding_queue_depth: 0, // TODO: Implement queue depth tracking
    })
}

pub async fn metrics(State(state): State<Arc<AppState>>) -> std::result::Result<String, (StatusCode, String)> {
    let metrics = Metrics::default();
    match metrics.format_prometheus(&state).await {
        Ok(output) => Ok(output),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn get_review_queue(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ReviewQuery>,
) -> std::result::Result<Json<Value>, (StatusCode, String)> {
    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    let domain_filter = query.domain.as_deref();
    
    // Get actual review queue items
    let review_items = crate::db::queries::get_review_queue(&state.pool, limit, offset, domain_filter)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    // Get total count for pagination
    let total = crate::db::queries::get_review_queue_count(&state.pool, domain_filter)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let items_json: Vec<Value> = review_items.iter().map(|item| json!({
        "atom_id": item.atom_id,
        "atom_type": item.atom_type,
        "domain": item.domain,
        "statement": item.statement,
        "author_agent_id": item.author_agent_id,
        "created_at": item.created_at,
        "review_status": item.review_status,
        "auto_review_eligible": item.auto_review_eligible
    })).collect();

    Ok(Json(json!({
        "items": items_json,
        "total": total,
        "limit": limit,
        "offset": offset
    })))
}

pub async fn review_atom(
    State(state): State<Arc<AppState>>,
    Path(atom_id): Path<String>,
    Json(action): Json<ReviewAction>,
) -> std::result::Result<Json<Value>, (StatusCode, String)> {
    // Validate action
    if !matches!(action.action.as_str(), "approve" | "reject") {
        return Err((StatusCode::BAD_REQUEST, "Invalid action. Must be 'approve' or 'reject'".to_string()));
    }

    // Check if atom exists
    let atom_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM atoms WHERE atom_id = $1)")
        .bind(&atom_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !atom_exists {
        return Err((StatusCode::NOT_FOUND, "Atom not found".to_string()));
    }

    // For testing, use the atom author as the reviewer (self-review)
    // TODO: Add proper authentication for reviewers
    let reviewer_agent_id: String = sqlx::query_scalar("SELECT author_agent_id FROM atoms WHERE atom_id = $1")
        .bind(&atom_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Create review record and update atom status
    let review_id = crate::db::queries::create_review(
        &state.pool,
        &atom_id,
        &reviewer_agent_id,
        &action.action,
        action.reason.as_deref(),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let response = json!({
        "status": action.action,
        "atom_id": atom_id,
        "review_id": review_id,
        "reason": action.reason.unwrap_or("Reviewed by system".to_string())
    });

    Ok(Json(response))
}

/// POST /admin/trigger-bounty-tick
///
/// Immediately runs one bounty-worker tick against the live database.
/// Intended for integration tests so they don't have to wait for the periodic timer.
pub async fn trigger_bounty_tick(
    State(state): State<Arc<AppState>>,
) -> std::result::Result<Json<Value>, (StatusCode, String)> {
    let worker = crate::workers::bounty::BountyWorker::new(
        state.pool.clone(),
        state.config.workers.bounty_needed_novelty_threshold,
        state.config.pheromone.exploration_samples,
        state.config.pheromone.exploration_density_radius,
        state.config.hub.embedding_dimension,
        state.config.workers.bounty_sparse_region_max_atoms,
    );

    match worker.run_bounty_tick().await {
        Ok(count) => Ok(Json(json!({ "status": "ok", "bounties_published": count }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

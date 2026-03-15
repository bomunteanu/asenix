use crate::db::queries;
use crate::error::MoteError;
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

// ── error mapping ─────────────────────────────────────────────────────────────

fn err(e: MoteError) -> (StatusCode, Json<Value>) {
    let status = match &e {
        MoteError::NotFound(_)     => StatusCode::NOT_FOUND,
        MoteError::Validation(_)   => StatusCode::BAD_REQUEST,
        MoteError::Conflict(_)     => StatusCode::CONFLICT,
        MoteError::Authentication(_) => StatusCode::UNAUTHORIZED,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (status, Json(json!({ "error": e.to_string() })))
}

// ── request bodies ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct SetRequirementsRequest {
    pub requirements: Value,
}

#[derive(Deserialize)]
pub struct SetSeedBountyRequest {
    pub seed_bounty: Value,
}

// ── filename validation ───────────────────────────────────────────────────────

fn validate_filename(filename: &str) -> Result<(), (StatusCode, Json<Value>)> {
    if filename.is_empty() || filename.len() > 255 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "filename must be between 1 and 255 characters" })),
        ));
    }
    if !filename
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "filename may only contain letters, digits, '.', '_', and '-'" })),
        ));
    }
    Ok(())
}

// ── POST /projects ────────────────────────────────────────────────────────────

pub async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let project = queries::create_project(
        &state.pool,
        &body.name,
        &body.slug,
        body.description.as_deref(),
    )
    .await
    .map_err(err)?;

    Ok((StatusCode::CREATED, Json(json!({
        "project_id":  project.project_id,
        "name":        project.name,
        "slug":        project.slug,
        "description": project.description,
        "created_at":  project.created_at,
    }))))
}

// ── GET /projects ─────────────────────────────────────────────────────────────

pub async fn list_projects(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let projects = queries::list_projects(&state.pool).await.map_err(err)?;
    let total = projects.len();
    let items: Vec<Value> = projects
        .into_iter()
        .map(|p| json!({
            "project_id":  p.project_id,
            "name":        p.name,
            "slug":        p.slug,
            "description": p.description,
            "created_at":  p.created_at,
        }))
        .collect();

    Ok(Json(json!({ "projects": items, "total": total })))
}

// ── GET /projects/:project_id ─────────────────────────────────────────────────

pub async fn get_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let p = queries::get_project(&state.pool, &project_id).await.map_err(err)?;
    Ok(Json(json!({
        "project_id":  p.project_id,
        "name":        p.name,
        "slug":        p.slug,
        "description": p.description,
        "created_at":  p.created_at,
    })))
}

// ── DELETE /projects/:project_id ──────────────────────────────────────────────

pub async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    queries::delete_project(&state.pool, &project_id).await.map_err(err)?;
    Ok(Json(json!({ "status": "deleted", "project_id": project_id })))
}

// ── PUT /projects/:project_id/protocol ───────────────────────────────────────

pub async fn set_protocol(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    body: bytes::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let text = String::from_utf8(body.to_vec()).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "protocol must be valid UTF-8" })),
        )
    })?;

    queries::set_protocol(&state.pool, &project_id, &text)
        .await
        .map_err(err)?;

    Ok(Json(json!({ "project_id": project_id, "updated": true })))
}

// ── GET /projects/:project_id/protocol ───────────────────────────────────────

pub async fn get_protocol(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    match queries::get_protocol(&state.pool, &project_id).await {
        Err(e) => Err(err(e)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no protocol set for this project" })),
        )),
        Ok(Some(text)) => Ok((
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            text,
        )
            .into_response()),
    }
}

// ── PUT /projects/:project_id/requirements ────────────────────────────────────

pub async fn set_requirements(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(body): Json<SetRequirementsRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let saved = queries::set_requirements(&state.pool, &project_id, &body.requirements)
        .await
        .map_err(err)?;

    Ok(Json(json!({ "project_id": project_id, "requirements": saved })))
}

// ── GET /projects/:project_id/requirements ────────────────────────────────────

pub async fn get_requirements(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let reqs = queries::get_requirements(&state.pool, &project_id)
        .await
        .map_err(err)?;

    Ok(Json(json!({ "project_id": project_id, "requirements": reqs })))
}

// ── PUT /projects/:project_id/seed-bounty ────────────────────────────────────

pub async fn set_seed_bounty(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
    Json(body): Json<SetSeedBountyRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    queries::set_seed_bounty(&state.pool, &project_id, &body.seed_bounty)
        .await
        .map_err(err)?;

    Ok(Json(json!({ "project_id": project_id, "updated": true })))
}

// ── GET /projects/:project_id/seed-bounty ────────────────────────────────────

pub async fn get_seed_bounty(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match queries::get_seed_bounty(&state.pool, &project_id).await {
        Err(e) => Err(err(e)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no seed bounty set for this project" })),
        )),
        Ok(Some(bounty)) => Ok(Json(json!({
            "project_id":  project_id,
            "seed_bounty": bounty,
        }))),
    }
}

// ── GET /projects/:project_id/files ──────────────────────────────────────────

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let files = queries::list_files(&state.pool, &project_id).await.map_err(err)?;
    let items: Vec<Value> = files
        .into_iter()
        .map(|f| json!({
            "filename":     f.filename,
            "size_bytes":   f.size_bytes,
            "content_type": f.content_type,
            "uploaded_at":  f.uploaded_at,
        }))
        .collect();

    Ok(Json(json!({ "project_id": project_id, "files": items })))
}

// ── PUT /projects/:project_id/files/:filename ─────────────────────────────────

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    Path((project_id, filename)): Path<(String, String)>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    validate_filename(&filename)?;

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let overwritten = queries::upload_file(
        &state.pool,
        &project_id,
        &filename,
        &body,
        content_type.as_deref(),
    )
    .await
    .map_err(err)?;

    Ok(Json(json!({
        "filename":   filename,
        "size_bytes": body.len(),
        "overwritten": overwritten,
    })))
}

// ── GET /projects/:project_id/files/:filename ─────────────────────────────────

pub async fn get_file(
    State(state): State<Arc<AppState>>,
    Path((project_id, filename)): Path<(String, String)>,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let (content, content_type) = queries::get_file(&state.pool, &project_id, &filename)
        .await
        .map_err(err)?;

    let ct = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
    Ok(([(header::CONTENT_TYPE, ct)], content).into_response())
}

// ── DELETE /projects/:project_id/files/:filename ──────────────────────────────

pub async fn delete_file(
    State(state): State<Arc<AppState>>,
    Path((project_id, filename)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    queries::delete_file(&state.pool, &project_id, &filename)
        .await
        .map_err(err)?;

    Ok(Json(json!({ "status": "deleted", "filename": filename })))
}

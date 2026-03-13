use crate::state::AppState;
use crate::storage::{StorageBackend, StorageError};
use crate::crypto::signing::verify_signature;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use blake3::Hasher;
use hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, Serialize)]
pub struct ArtifactUploadResponse {
    pub status: String,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactMetadata {
    pub hash: String,
    pub type_: String,
    pub size_bytes: i64,
    pub media_type: Option<String>,
    pub uploaded_by: String,
    pub uploaded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub hash: String,
    pub type_: String,
}

#[derive(Debug, Deserialize)]
pub struct TreeManifest {
    pub entries: Vec<TreeEntry>,
}

pub async fn put_artifact(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
    headers: HeaderMap,
    body: bytes::Bytes,
) -> Result<Json<ArtifactUploadResponse>, StatusCode> {
    // Extract required headers
    let artifact_type = headers
        .get("X-Artifact-Type")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    let agent_id = headers
        .get("X-Agent-Id")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let signature_str = headers
        .get("X-Signature")
        .and_then(|h| h.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Validate artifact type
    if artifact_type != "blob" && artifact_type != "tree" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Verify agent exists and is confirmed
    let agent_row = sqlx::query(
        "SELECT public_key FROM agents WHERE agent_id = $1 AND confirmed = true"
    )
    .bind(&agent_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error checking agent: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let agent_public_key_bytes: Vec<u8> = agent_row
        .ok_or(StatusCode::UNAUTHORIZED)?
        .get("public_key");

    // Verify signature of hash
    let signature_bytes = hex::decode(signature_str)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    
    verify_signature(
        &agent_public_key_bytes,
        hash.as_bytes(),
        &signature_bytes
    ).map_err(|_| StatusCode::UNAUTHORIZED)?;

    // Check rate limits
    let rate_check = sqlx::query(
        "SELECT COUNT(*) as count FROM atoms WHERE author_agent_id = $1 AND created_at > NOW() - INTERVAL '1 hour'"
    )
    .bind(&agent_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error checking rate limits: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let count: i64 = rate_check.get("count");
    if count as usize >= state.config.trust.max_atoms_per_hour {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    // Check if artifact already exists
    let existing = sqlx::query(
        "SELECT hash FROM artifacts WHERE hash = $1"
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error checking existing artifact: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if existing.is_some() {
        return Ok(Json(ArtifactUploadResponse {
            status: "exists".to_string(),
            hash,
            size_bytes: None,
        }));
    }

    // Compute BLAKE3 hash of body
    let mut hasher = Hasher::new();
    hasher.update(&body);
    let computed_hash = hex::encode(hasher.finalize().as_bytes());
    if computed_hash != hash {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check size limits for blobs
    if artifact_type == "blob" {
        let body_size = body.len() as u64;
        if body_size > state.config.hub.max_artifact_blob_bytes {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }

        // Check per-agent storage limit
        let storage_check = sqlx::query(
            "SELECT COALESCE(SUM(size_bytes), 0) as total FROM artifacts WHERE uploaded_by = $1"
        )
        .bind(&agent_id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error checking storage limits: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let total_storage: i64 = storage_check.get("total");
        if (total_storage as u64 + body_size) > state.config.hub.max_artifact_storage_per_agent_bytes {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }

    // Validate tree manifest if type is tree
    if artifact_type == "tree" {
        let manifest_str = String::from_utf8(body.to_vec())
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        
        let manifest: Value = serde_json::from_str(&manifest_str)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        
        let entries = manifest.as_array()
            .ok_or(StatusCode::BAD_REQUEST)?;
        
        for entry in entries {
            let name = entry.get("name")
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            
            let hash = entry.get("hash")
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            
            let type_ = entry.get("type")
                .and_then(|v| v.as_str())
                .ok_or(StatusCode::BAD_REQUEST)?;
            
            if type_ != "blob" && type_ != "tree" {
                return Err(StatusCode::BAD_REQUEST);
            }
            
            // Validate name and hash are non-empty
            if name.is_empty() || hash.is_empty() {
                return Err(StatusCode::BAD_REQUEST);
            }
        }
    }

    // Store to backend
    state.storage.put(&hash, body.to_vec()).await
        .map_err(|e| {
            tracing::error!("Storage error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Insert into database
    let media_type = if artifact_type == "blob" {
        headers
            .get("Content-Type")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string())
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO artifacts (hash, type, size_bytes, media_type, uploaded_by) 
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(&hash)
    .bind(artifact_type)
    .bind(body.len() as i64)
    .bind(media_type)
    .bind(&agent_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error inserting artifact: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ArtifactUploadResponse {
        status: "created".to_string(),
        hash,
        size_bytes: Some(body.len() as i64),
    }))
}

pub async fn get_artifact(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<Response, StatusCode> {
    // Look up artifact metadata
    let row = sqlx::query(
        "SELECT type, media_type FROM artifacts WHERE hash = $1"
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error fetching artifact: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let (artifact_type, media_type): (String, Option<String>) = row
        .map(|r| (r.get("type"), r.get("media_type")))
        .ok_or(StatusCode::NOT_FOUND)?;

    // Retrieve from storage
    let data = state.storage.get(&hash).await
        .map_err(|e| {
            tracing::error!("Storage error: {}", e);
            if matches!(e, StorageError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    // Set appropriate content type
    let content_type = if artifact_type == "blob" {
        media_type.unwrap_or_else(|| "application/octet-stream".to_string())
    } else {
        "application/json".to_string()
    };

    Ok((
        StatusCode::OK,
        [("Content-Type", content_type)],
        data,
    ).into_response())
}

pub async fn head_artifact(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> StatusCode {
    let exists = sqlx::query(
        "SELECT 1 FROM artifacts WHERE hash = $1"
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error checking artifact: {}", e);
    });

    match exists {
        Ok(Some(_)) => StatusCode::OK,
        Ok(None) => StatusCode::NOT_FOUND,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

pub async fn get_artifact_metadata(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<Json<ArtifactMetadata>, StatusCode> {
    let row = sqlx::query(
        "SELECT type, size_bytes, media_type, uploaded_by, uploaded_at 
         FROM artifacts WHERE hash = $1"
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error fetching artifact metadata: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let row = row.ok_or(StatusCode::NOT_FOUND)?;

    let metadata = ArtifactMetadata {
        hash,
        type_: row.get("type"),
        size_bytes: row.get("size_bytes"),
        media_type: row.get("media_type"),
        uploaded_by: row.get("uploaded_by"),
        uploaded_at: row.get("uploaded_at"),
    };

    Ok(Json(metadata))
}

pub async fn list_artifact_tree(
    State(state): State<Arc<AppState>>,
    Path(hash): Path<String>,
) -> Result<Json<Vec<TreeEntry>>, StatusCode> {
    // Check if artifact exists and is a tree
    let row = sqlx::query(
        "SELECT type FROM artifacts WHERE hash = $1"
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error checking artifact type: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let artifact_type: String = row
        .map(|r| r.get("type"))
        .ok_or(StatusCode::NOT_FOUND)?;

    if artifact_type != "tree" {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Get tree manifest
    let data = state.storage.get(&hash).await
        .map_err(|e| {
            tracing::error!("Storage error: {}", e);
            if matches!(e, StorageError::NotFound(_)) {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        })?;

    let manifest: Value = serde_json::from_slice(&data)
        .map_err(|e| {
            tracing::error!("JSON parsing error: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let entries = manifest.as_array()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?
        .iter()
        .map(|entry| TreeEntry {
            name: entry.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            hash: entry.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            type_: entry.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        })
        .collect();

    Ok(Json(entries))
}

pub async fn resolve_artifact_path(
    State(state): State<Arc<AppState>>,
    Path((hash, path)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    // Split path by '/'
    let path_segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    
    if path_segments.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut current_hash = hash;

    // Walk the tree hierarchy
    for (i, segment) in path_segments.iter().enumerate() {
        // Get current tree metadata
        let row = sqlx::query(
            "SELECT type FROM artifacts WHERE hash = $1"
        )
        .bind(&current_hash)
        .fetch_optional(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error checking artifact type: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let artifact_type: String = row
            .map(|r| r.get("type"))
            .ok_or(StatusCode::NOT_FOUND)?;

        if artifact_type != "tree" {
            // If this is not the last segment, we can't traverse into a blob
            if i < path_segments.len() - 1 {
                return Err(StatusCode::NOT_FOUND);
            }
            
            // This is the last segment and it's a blob, return it
            let data = state.storage.get(&current_hash).await
                .map_err(|e| {
                    tracing::error!("Storage error: {}", e);
                    if matches!(e, StorageError::NotFound(_)) {
                        StatusCode::NOT_FOUND
                    } else {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                })?;

            // Get media type for blob
            let media_type_row = sqlx::query(
                "SELECT media_type FROM artifacts WHERE hash = $1"
            )
            .bind(&current_hash)
            .fetch_one(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("Database error fetching media type: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

            let media_type: Option<String> = media_type_row.get("media_type");
            let content_type = media_type.unwrap_or_else(|| "application/octet-stream".to_string());

            return Ok((
                StatusCode::OK,
                [("Content-Type", content_type)],
                data,
            ).into_response());
        }

        // Get tree manifest and find the entry
        let data = state.storage.get(&current_hash).await
            .map_err(|e| {
                tracing::error!("Storage error: {}", e);
                if matches!(e, StorageError::NotFound(_)) {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
            })?;

        let manifest: Value = serde_json::from_slice(&data)
            .map_err(|e| {
                tracing::error!("JSON parsing error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        let entries = manifest.as_array()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

        let mut found = false;
        for entry in entries {
            let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name == *segment {
                current_hash = entry.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
                found = true;
                break;
            }
        }

        if !found {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    // If we get here, we should have resolved to a blob
    Err(StatusCode::NOT_FOUND)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tree_manifest_validation() {
        // Valid manifest
        let valid = json!([
            {"name": "file.txt", "hash": "abcd1234", "type": "blob"},
            {"name": "subdir", "hash": "efgh5678", "type": "tree"}
        ]);
        assert!(valid.is_array());

        // Invalid manifest - missing field
        let invalid = json!([
            {"name": "file.txt", "hash": "abcd1234"}  // missing type
        ]);
        assert!(invalid.is_array());
    }
}

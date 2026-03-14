use crate::storage::{StorageBackend};
use crate::error::MoteError;
use blake3::Hasher;
use hex;
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Serde module: serializes Vec<u8> as a base64 string on the wire.
/// Agents send `"data": "<base64>"` — no integer arrays, no Blob wrapper tag.
mod base64_serde {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserializer, Serializer};
    use serde::de::Error;

    pub fn serialize<S: Serializer>(data: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(data))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s: String = serde::Deserialize::deserialize(d)?;
        STANDARD.decode(&s).map_err(|e| D::Error::custom(format!("invalid base64: {e}")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineArtifact {
    pub artifact_type: String,
    pub content: ArtifactContent,
    pub media_type: Option<String>,
}

/// Wire format uses untagged serde so agents send:
///   blob → `{"data": "<base64>"}`
///   tree → `{"entries": [...]}`
/// No "Blob"/"Tree" wrapper key, no integer arrays.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArtifactContent {
    Blob {
        #[serde(with = "base64_serde")]
        data: Vec<u8>,
    },
    Tree { entries: Vec<TreeEntry> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    pub hash: String,
    pub type_: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeManifest {
    pub entries: Vec<TreeEntry>,
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

#[derive(Debug, Serialize)]
pub struct ArtifactUploadResponse {
    pub status: String,
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<i64>,
}

/// Process an inline artifact and return its hash
pub async fn process_inline_artifact(
    pool: &sqlx::PgPool,
    storage: &dyn StorageBackend,
    agent_id: &str,
    artifact: InlineArtifact,
) -> Result<String, MoteError> {
    // Validate artifact type
    if artifact.artifact_type != "blob" && artifact.artifact_type != "tree" {
        return Err(MoteError::Validation(
            "artifact_type must be 'blob' or 'tree'".to_string()
        ));
    }

    // Compute hash and prepare content
    let (hash, content_bytes) = match artifact.content {
        ArtifactContent::Blob { data } => {
            let mut hasher = Hasher::new();
            hasher.update(&data);
            let hash = hex::encode(hasher.finalize().as_bytes());
            (hash, data)
        }
        ArtifactContent::Tree { entries } => {
            // Validate tree entries
            for entry in &entries {
                if entry.type_ != "blob" && entry.type_ != "tree" {
                    return Err(MoteError::Validation(
                        format!("Tree entry {} has invalid type: {}", entry.name, entry.type_)
                    ));
                }
                if entry.name.is_empty() || entry.hash.is_empty() {
                    return Err(MoteError::Validation(
                        "Tree entries must have non-empty name and hash".to_string()
                    ));
                }
            }
            
            let manifest = TreeManifest { entries };
            let manifest_json = serde_json::to_string(&manifest)
                .map_err(|e| MoteError::Validation(format!("Invalid tree manifest: {}", e)))?;
            
            let mut hasher = Hasher::new();
            hasher.update(manifest_json.as_bytes());
            let hash = hex::encode(hasher.finalize().as_bytes());
            (hash, manifest_json.into_bytes())
        }
    };

    // Check if artifact already exists
    let existing = sqlx::query("SELECT hash FROM artifacts WHERE hash = $1")
        .bind(&hash)
        .fetch_optional(pool)
        .await
        .map_err(MoteError::Database)?;

    if existing.is_some() {
        return Ok(hash); // Artifact already exists, return existing hash
    }

    // Check size limits for blobs
    if artifact.artifact_type == "blob" {
        let content_size = content_bytes.len() as u64;
        
        // Check per-agent storage limit
        let storage_check = sqlx::query(
            "SELECT COALESCE(SUM(size_bytes), 0)::bigint as total FROM artifacts WHERE uploaded_by = $1"
        )
        .bind(agent_id)
        .fetch_one(pool)
        .await
        .map_err(MoteError::Database)?;

        let total_storage: i64 = storage_check.get("total");
        let max_storage_per_agent = 10 * 1024 * 1024; // 10MB default
        if (total_storage as u64 + content_size) > max_storage_per_agent {
            return Err(MoteError::Validation(
                "Agent storage limit exceeded".to_string()
            ));
        }
    }

    // Store to backend
    storage.put(&hash, content_bytes.clone()).await
        .map_err(|e| MoteError::Storage(format!("Storage error: {}", e)))?;

    // Insert into database
    let media_type = if artifact.artifact_type == "blob" {
        artifact.media_type
    } else {
        None
    };

    sqlx::query(
        "INSERT INTO artifacts (hash, type, size_bytes, media_type, uploaded_by) 
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(&hash)
    .bind(&artifact.artifact_type)
    .bind(content_bytes.len() as i64)
    .bind(media_type)
    .bind(agent_id)
    .execute(pool)
    .await
    .map_err(MoteError::Database)?;

    Ok(hash)
}

/// Get artifact metadata
pub async fn get_artifact_metadata(
    pool: &sqlx::PgPool,
    hash: &str,
) -> Result<ArtifactMetadata, MoteError> {
    let row = sqlx::query(
        "SELECT hash, type, size_bytes, media_type, uploaded_by, uploaded_at 
         FROM artifacts WHERE hash = $1"
    )
    .bind(hash)
    .fetch_optional(pool)
    .await
    .map_err(MoteError::Database)?;

    let row = row.ok_or_else(|| MoteError::Validation(
        "Artifact not found".to_string()
    ))?;

    Ok(ArtifactMetadata {
        hash: row.get("hash"),
        type_: row.get("type"),
        size_bytes: row.get("size_bytes"),
        media_type: row.get("media_type"),
        uploaded_by: row.get("uploaded_by"),
        uploaded_at: row.get("uploaded_at"),
    })
}

/// Download artifact content
pub async fn download_artifact(
    pool: &sqlx::PgPool,
    storage: &dyn StorageBackend,
    hash: &str,
) -> Result<(ArtifactMetadata, Vec<u8>), MoteError> {
    // Get metadata first
    let metadata = get_artifact_metadata(pool, hash).await?;

    // Get content from storage
    let content = storage.get(hash).await
        .map_err(|e| MoteError::Storage(format!("Storage error: {}", e)))?;

    Ok((metadata, content))
}

/// List artifacts with optional filtering
pub async fn list_artifacts(
    pool: &sqlx::PgPool,
    artifact_type: Option<&str>,
    uploaded_by: Option<&str>,
    limit: Option<i64>,
) -> Result<Vec<ArtifactMetadata>, MoteError> {
    let mut query = "SELECT hash, type, size_bytes, media_type, uploaded_by, uploaded_at FROM artifacts".to_string();
    let mut where_clauses = Vec::new();

    if artifact_type.is_some() {
        where_clauses.push(format!("type = ${}", where_clauses.len() + 1));
    }

    if uploaded_by.is_some() {
        where_clauses.push(format!("uploaded_by = ${}", where_clauses.len() + 1));
    }

    if !where_clauses.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&where_clauses.join(" AND "));
    }

    query.push_str(" ORDER BY uploaded_at DESC");

    if let Some(limit_val) = limit {
        query.push_str(&format!(" LIMIT {}", limit_val));
    }

    let mut sql_query = sqlx::query(&query);

    if let Some(artifact_type_val) = artifact_type {
        sql_query = sql_query.bind(artifact_type_val);
    }

    if let Some(uploaded_by_val) = uploaded_by {
        sql_query = sql_query.bind(uploaded_by_val);
    }

    let rows = sql_query.fetch_all(pool).await
        .map_err(MoteError::Database)?;

    let artifacts = rows.iter().map(|row| ArtifactMetadata {
        hash: row.get("hash"),
        type_: row.get("type"),
        size_bytes: row.get("size_bytes"),
        media_type: row.get("media_type"),
        uploaded_by: row.get("uploaded_by"),
        uploaded_at: row.get("uploaded_at"),
    }).collect();

    Ok(artifacts)
}

/// Delete an artifact
pub async fn delete_artifact(
    pool: &sqlx::PgPool,
    storage: &dyn StorageBackend,
    hash: &str,
    agent_id: &str,
) -> Result<(), MoteError> {
    // Check if artifact exists and belongs to agent
    let row = sqlx::query("SELECT uploaded_by FROM artifacts WHERE hash = $1")
        .bind(hash)
        .fetch_optional(pool)
        .await
        .map_err(MoteError::Database)?;

    let uploaded_by: String = row.ok_or_else(|| MoteError::Validation(
        "Artifact not found".to_string()
    ))?.get("uploaded_by");

    if uploaded_by != agent_id {
        return Err(MoteError::Validation(
            "Can only delete own artifacts".to_string()
        ));
    }

    // Check if any atoms reference this artifact
    let atom_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM atoms WHERE artifact_tree_hash = $1")
        .bind(hash)
        .fetch_one(pool)
        .await
        .map_err(MoteError::Database)?;

    if atom_count > 0 {
        return Err(MoteError::Validation(
            "Cannot delete artifact referenced by atoms".to_string()
        ));
    }

    // Delete from storage
    storage.delete(hash).await
        .map_err(|e| MoteError::Storage(format!("Storage error: {}", e)))?;

    // Delete from database
    sqlx::query("DELETE FROM artifacts WHERE hash = $1")
        .bind(hash)
        .execute(pool)
        .await
        .map_err(MoteError::Database)?;

    Ok(())
}

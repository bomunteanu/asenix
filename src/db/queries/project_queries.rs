use crate::error::{MoteError, Result};
use crate::domain::project::{Project, ProjectFile};
use sqlx::{PgPool, Row};
use uuid::Uuid;

// ── helpers ───────────────────────────────────────────────────────────────────

fn validate_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(MoteError::Validation("slug must not be empty".to_string()));
    }
    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(MoteError::Validation(
            "slug must contain only lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    Ok(())
}

fn slug_conflict(e: sqlx::Error, slug: &str) -> MoteError {
    let msg = e.to_string();
    if msg.contains("unique") || msg.contains("duplicate") {
        MoteError::Conflict(format!("slug '{}' is already taken", slug))
    } else {
        MoteError::Database(e)
    }
}

fn project_from_row(row: sqlx::postgres::PgRow) -> Project {
    Project {
        project_id: row.get("project_id"),
        name: row.get("name"),
        slug: row.get("slug"),
        description: row.get("description"),
        created_at: row.get("created_at"),
    }
}

// ── project CRUD ──────────────────────────────────────────────────────────────

pub async fn create_project(
    pool: &PgPool,
    name: &str,
    slug: &str,
    description: Option<&str>,
) -> Result<Project> {
    validate_slug(slug)?;

    let project_id = format!("proj_{}", Uuid::new_v4().to_string().replace('-', ""));

    let row = sqlx::query(
        "INSERT INTO projects (project_id, name, slug, description, created_at)
         VALUES ($1, $2, $3, $4, NOW())
         RETURNING project_id, name, slug, description, created_at",
    )
    .bind(&project_id)
    .bind(name)
    .bind(slug)
    .bind(description)
    .fetch_one(pool)
    .await
    .map_err(|e| slug_conflict(e, slug))?;

    Ok(project_from_row(row))
}

pub async fn list_projects(pool: &PgPool) -> Result<Vec<Project>> {
    let rows = sqlx::query(
        "SELECT project_id, name, slug, description, created_at
         FROM projects
         ORDER BY created_at ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(MoteError::Database)?;

    Ok(rows.into_iter().map(project_from_row).collect())
}

pub async fn get_project(pool: &PgPool, project_id: &str) -> Result<Project> {
    let row = sqlx::query(
        "SELECT project_id, name, slug, description, created_at
         FROM projects WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(MoteError::Database)?
    .ok_or_else(|| MoteError::NotFound(format!("project '{}' not found", project_id)))?;

    Ok(project_from_row(row))
}

pub async fn get_project_by_slug(pool: &PgPool, slug: &str) -> Result<Project> {
    let row = sqlx::query(
        "SELECT project_id, name, slug, description, created_at
         FROM projects WHERE slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await
    .map_err(MoteError::Database)?
    .ok_or_else(|| MoteError::NotFound(format!("project with slug '{}' not found", slug)))?;

    Ok(project_from_row(row))
}

pub async fn update_project(
    pool: &PgPool,
    project_id: &str,
    name: &str,
    slug: &str,
    description: Option<&str>,
) -> Result<Project> {
    validate_slug(slug)?;

    let row = sqlx::query(
        "UPDATE projects SET name = $1, slug = $2, description = $3
         WHERE project_id = $4
         RETURNING project_id, name, slug, description, created_at",
    )
    .bind(name)
    .bind(slug)
    .bind(description)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| slug_conflict(e, slug))?
    .ok_or_else(|| MoteError::NotFound(format!("project '{}' not found", project_id)))?;

    Ok(project_from_row(row))
}

pub async fn delete_project(pool: &PgPool, project_id: &str) -> Result<()> {
    sqlx::query("UPDATE atoms SET project_id = NULL WHERE project_id = $1")
        .bind(project_id)
        .execute(pool)
        .await
        .map_err(MoteError::Database)?;

    let result = sqlx::query("DELETE FROM projects WHERE project_id = $1")
        .bind(project_id)
        .execute(pool)
        .await
        .map_err(MoteError::Database)?;

    if result.rows_affected() == 0 {
        return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
    }
    Ok(())
}

// ── protocol ──────────────────────────────────────────────────────────────────

/// Returns `Ok(None)` when the project exists but has no protocol set.
/// Returns `Err(NotFound)` when the project itself does not exist.
pub async fn get_protocol(pool: &PgPool, project_id: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT protocol FROM projects WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(MoteError::Database)?;

    match row {
        None => Err(MoteError::NotFound(format!("project '{}' not found", project_id))),
        Some(r) => Ok(r.get("protocol")),
    }
}

pub async fn set_protocol(pool: &PgPool, project_id: &str, protocol: &str) -> Result<()> {
    let result = sqlx::query("UPDATE projects SET protocol = $1 WHERE project_id = $2")
        .bind(protocol)
        .bind(project_id)
        .execute(pool)
        .await
        .map_err(MoteError::Database)?;

    if result.rows_affected() == 0 {
        return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
    }
    Ok(())
}

// ── requirements ──────────────────────────────────────────────────────────────

pub async fn get_requirements(pool: &PgPool, project_id: &str) -> Result<serde_json::Value> {
    let row = sqlx::query("SELECT requirements FROM projects WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(MoteError::Database)?;

    match row {
        None => Err(MoteError::NotFound(format!("project '{}' not found", project_id))),
        Some(r) => Ok(r.get::<serde_json::Value, _>("requirements")),
    }
}

pub async fn set_requirements(
    pool: &PgPool,
    project_id: &str,
    requirements: &serde_json::Value,
) -> Result<serde_json::Value> {
    let row = sqlx::query(
        "UPDATE projects SET requirements = $1 WHERE project_id = $2
         RETURNING requirements",
    )
    .bind(requirements)
    .bind(project_id)
    .fetch_optional(pool)
    .await
    .map_err(MoteError::Database)?
    .ok_or_else(|| MoteError::NotFound(format!("project '{}' not found", project_id)))?;

    Ok(row.get::<serde_json::Value, _>("requirements"))
}

// ── seed bounty ───────────────────────────────────────────────────────────────

/// Returns `Ok(None)` when the project exists but has no seed bounty set.
/// Returns `Err(NotFound)` when the project itself does not exist.
pub async fn get_seed_bounty(pool: &PgPool, project_id: &str) -> Result<Option<serde_json::Value>> {
    let row = sqlx::query("SELECT seed_bounty FROM projects WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await
        .map_err(MoteError::Database)?;

    match row {
        None => Err(MoteError::NotFound(format!("project '{}' not found", project_id))),
        Some(r) => Ok(r.get::<Option<serde_json::Value>, _>("seed_bounty")),
    }
}

pub async fn set_seed_bounty(
    pool: &PgPool,
    project_id: &str,
    seed_bounty: &serde_json::Value,
) -> Result<()> {
    let result =
        sqlx::query("UPDATE projects SET seed_bounty = $1 WHERE project_id = $2")
            .bind(seed_bounty)
            .bind(project_id)
            .execute(pool)
            .await
            .map_err(MoteError::Database)?;

    if result.rows_affected() == 0 {
        return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
    }
    Ok(())
}

// ── files ─────────────────────────────────────────────────────────────────────

/// Returns `true` if an existing file was overwritten.
pub async fn upload_file(
    pool: &PgPool,
    project_id: &str,
    filename: &str,
    content: &[u8],
    content_type: Option<&str>,
) -> Result<bool> {
    let project_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE project_id = $1)")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .map_err(MoteError::Database)?;

    if !project_exists {
        return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
    }

    let already_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM project_files WHERE project_id = $1 AND filename = $2)",
    )
    .bind(project_id)
    .bind(filename)
    .fetch_one(pool)
    .await
    .map_err(MoteError::Database)?;

    sqlx::query(
        "INSERT INTO project_files (project_id, filename, content, size_bytes, content_type, uploaded_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         ON CONFLICT (project_id, filename) DO UPDATE SET
             content      = EXCLUDED.content,
             size_bytes   = EXCLUDED.size_bytes,
             content_type = EXCLUDED.content_type,
             uploaded_at  = NOW()",
    )
    .bind(project_id)
    .bind(filename)
    .bind(content)
    .bind(content.len() as i32)
    .bind(content_type)
    .execute(pool)
    .await
    .map_err(MoteError::Database)?;

    Ok(already_exists)
}

pub async fn get_file(
    pool: &PgPool,
    project_id: &str,
    filename: &str,
) -> Result<(Vec<u8>, Option<String>)> {
    let project_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE project_id = $1)")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .map_err(MoteError::Database)?;

    if !project_exists {
        return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
    }

    let row = sqlx::query(
        "SELECT content, content_type FROM project_files
         WHERE project_id = $1 AND filename = $2",
    )
    .bind(project_id)
    .bind(filename)
    .fetch_optional(pool)
    .await
    .map_err(MoteError::Database)?
    .ok_or_else(|| MoteError::NotFound(format!("file '{}' not found", filename)))?;

    let content: Vec<u8> = row.get("content");
    let content_type: Option<String> = row.get("content_type");
    Ok((content, content_type))
}

pub async fn list_files(pool: &PgPool, project_id: &str) -> Result<Vec<ProjectFile>> {
    let project_exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE project_id = $1)")
            .bind(project_id)
            .fetch_one(pool)
            .await
            .map_err(MoteError::Database)?;

    if !project_exists {
        return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
    }

    let rows = sqlx::query(
        "SELECT filename, size_bytes, content_type, uploaded_at
         FROM project_files WHERE project_id = $1
         ORDER BY filename ASC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(MoteError::Database)?;

    Ok(rows
        .into_iter()
        .map(|r| ProjectFile {
            filename: r.get("filename"),
            size_bytes: r.get("size_bytes"),
            content_type: r.get("content_type"),
            uploaded_at: r.get("uploaded_at"),
        })
        .collect())
}

pub async fn delete_file(pool: &PgPool, project_id: &str, filename: &str) -> Result<()> {
    let result = sqlx::query(
        "DELETE FROM project_files WHERE project_id = $1 AND filename = $2",
    )
    .bind(project_id)
    .bind(filename)
    .execute(pool)
    .await
    .map_err(MoteError::Database)?;

    if result.rows_affected() == 0 {
        // Distinguish project-not-found from file-not-found
        let project_exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM projects WHERE project_id = $1)")
                .bind(project_id)
                .fetch_one(pool)
                .await
                .map_err(MoteError::Database)?;

        if !project_exists {
            return Err(MoteError::NotFound(format!("project '{}' not found", project_id)));
        }
        return Err(MoteError::NotFound(format!("file '{}' not found", filename)));
    }
    Ok(())
}

use crate::error::{MoteError, Result};
use crate::domain::project::Project;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn create_project(
    pool: &PgPool,
    name: &str,
    slug: &str,
    description: Option<&str>,
) -> Result<Project> {
    let project_id = format!("proj_{}", Uuid::new_v4().to_string().replace('-', ""));

    // Validate slug: lowercase alphanumeric + hyphens only
    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(MoteError::Validation(
            "slug must contain only lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    if slug.is_empty() {
        return Err(MoteError::Validation("slug must not be empty".to_string()));
    }

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
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            MoteError::Validation(format!("slug '{}' is already taken", slug))
        } else {
            MoteError::Database(e)
        }
    })?;

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
         FROM projects
         WHERE project_id = $1",
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
         FROM projects
         WHERE slug = $1",
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
    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err(MoteError::Validation(
            "slug must contain only lowercase letters, digits, and hyphens".to_string(),
        ));
    }
    if slug.is_empty() {
        return Err(MoteError::Validation("slug must not be empty".to_string()));
    }

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
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            MoteError::Validation(format!("slug '{}' is already taken", slug))
        } else {
            MoteError::Database(e)
        }
    })?
    .ok_or_else(|| MoteError::NotFound(format!("project '{}' not found", project_id)))?;

    Ok(project_from_row(row))
}

pub async fn delete_project(pool: &PgPool, project_id: &str) -> Result<()> {
    // Disassociate atoms from the project before deleting (preserve atoms, remove the link)
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

fn project_from_row(row: sqlx::postgres::PgRow) -> Project {
    Project {
        project_id: row.get("project_id"),
        name: row.get("name"),
        slug: row.get("slug"),
        description: row.get("description"),
        created_at: row.get("created_at"),
    }
}

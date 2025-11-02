use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::{ActiveJobset, CreateJobset, Jobset, UpdateJobset};

pub async fn create(pool: &PgPool, input: CreateJobset) -> Result<Jobset> {
    let enabled = input.enabled.unwrap_or(true);
    let flake_mode = input.flake_mode.unwrap_or(true);
    let check_interval = input.check_interval.unwrap_or(60);

    sqlx::query_as::<_, Jobset>(
        "INSERT INTO jobsets (project_id, name, nix_expression, enabled, flake_mode, check_interval) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(input.project_id)
    .bind(&input.name)
    .bind(&input.nix_expression)
    .bind(enabled)
    .bind(flake_mode)
    .bind(check_interval)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            CiError::Conflict(format!("Jobset '{}' already exists in this project", input.name))
        }
        _ => CiError::Database(e),
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Jobset> {
    sqlx::query_as::<_, Jobset>("SELECT * FROM jobsets WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CiError::NotFound(format!("Jobset {id} not found")))
}

pub async fn list_for_project(
    pool: &PgPool,
    project_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Jobset>> {
    sqlx::query_as::<_, Jobset>(
        "SELECT * FROM jobsets WHERE project_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn count_for_project(pool: &PgPool, project_id: Uuid) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets WHERE project_id = $1")
        .bind(project_id)
        .fetch_one(pool)
        .await
        .map_err(CiError::Database)?;
    Ok(row.0)
}

pub async fn update(pool: &PgPool, id: Uuid, input: UpdateJobset) -> Result<Jobset> {
    let existing = get(pool, id).await?;

    let name = input.name.unwrap_or(existing.name);
    let nix_expression = input.nix_expression.unwrap_or(existing.nix_expression);
    let enabled = input.enabled.unwrap_or(existing.enabled);
    let flake_mode = input.flake_mode.unwrap_or(existing.flake_mode);
    let check_interval = input.check_interval.unwrap_or(existing.check_interval);

    sqlx::query_as::<_, Jobset>(
        "UPDATE jobsets SET name = $1, nix_expression = $2, enabled = $3, flake_mode = $4, check_interval = $5 WHERE id = $6 RETURNING *",
    )
    .bind(&name)
    .bind(&nix_expression)
    .bind(enabled)
    .bind(flake_mode)
    .bind(check_interval)
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            CiError::Conflict(format!("Jobset '{name}' already exists in this project"))
        }
        _ => CiError::Database(e),
    })
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query("DELETE FROM jobsets WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(CiError::NotFound(format!("Jobset {id} not found")));
    }

    Ok(())
}

pub async fn upsert(pool: &PgPool, input: CreateJobset) -> Result<Jobset> {
    let enabled = input.enabled.unwrap_or(true);
    let flake_mode = input.flake_mode.unwrap_or(true);
    let check_interval = input.check_interval.unwrap_or(60);

    sqlx::query_as::<_, Jobset>(
        "INSERT INTO jobsets (project_id, name, nix_expression, enabled, flake_mode, check_interval) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (project_id, name) DO UPDATE SET \
         nix_expression = EXCLUDED.nix_expression, \
         enabled = EXCLUDED.enabled, \
         flake_mode = EXCLUDED.flake_mode, \
         check_interval = EXCLUDED.check_interval \
         RETURNING *",
    )
    .bind(input.project_id)
    .bind(&input.name)
    .bind(&input.nix_expression)
    .bind(enabled)
    .bind(flake_mode)
    .bind(check_interval)
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn list_active(pool: &PgPool) -> Result<Vec<ActiveJobset>> {
    sqlx::query_as::<_, ActiveJobset>("SELECT * FROM active_jobsets")
        .fetch_all(pool)
        .await
        .map_err(CiError::Database)
}

use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::{BuildStep, CreateBuildStep};

pub async fn create(pool: &PgPool, input: CreateBuildStep) -> Result<BuildStep> {
    sqlx::query_as::<_, BuildStep>(
        "INSERT INTO build_steps (build_id, step_number, command) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(input.build_id)
    .bind(input.step_number)
    .bind(&input.command)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            CiError::Conflict(format!(
                "Build step {} already exists for this build",
                input.step_number
            ))
        }
        _ => CiError::Database(e),
    })
}

pub async fn complete(
    pool: &PgPool,
    id: Uuid,
    exit_code: i32,
    output: Option<&str>,
    error_output: Option<&str>,
) -> Result<BuildStep> {
    sqlx::query_as::<_, BuildStep>(
        "UPDATE build_steps SET completed_at = NOW(), exit_code = $1, output = $2, error_output = $3 WHERE id = $4 RETURNING *",
    )
    .bind(exit_code)
    .bind(output)
    .bind(error_output)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Build step {id} not found")))
}

pub async fn list_for_build(pool: &PgPool, build_id: Uuid) -> Result<Vec<BuildStep>> {
    sqlx::query_as::<_, BuildStep>(
        "SELECT * FROM build_steps WHERE build_id = $1 ORDER BY step_number ASC",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

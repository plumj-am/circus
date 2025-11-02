use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::JobsetInput;

pub async fn create(
    pool: &PgPool,
    jobset_id: Uuid,
    name: &str,
    input_type: &str,
    value: &str,
    revision: Option<&str>,
) -> Result<JobsetInput> {
    sqlx::query_as::<_, JobsetInput>(
        "INSERT INTO jobset_inputs (jobset_id, name, input_type, value, revision) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(jobset_id)
    .bind(name)
    .bind(input_type)
    .bind(value)
    .bind(revision)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            CiError::Conflict(format!("Input '{name}' already exists in this jobset"))
        }
        _ => CiError::Database(e),
    })
}

pub async fn list_for_jobset(pool: &PgPool, jobset_id: Uuid) -> Result<Vec<JobsetInput>> {
    sqlx::query_as::<_, JobsetInput>(
        "SELECT * FROM jobset_inputs WHERE jobset_id = $1 ORDER BY name ASC",
    )
    .bind(jobset_id)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query("DELETE FROM jobset_inputs WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(CiError::NotFound(format!("Jobset input {id} not found")));
    }
    Ok(())
}

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateEvaluation, Evaluation, EvaluationStatus},
};

pub async fn create(
  pool: &PgPool,
  input: CreateEvaluation,
) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>(
    "INSERT INTO evaluations (jobset_id, commit_hash, status, pr_number, \
     pr_head_branch, pr_base_branch, pr_action) VALUES ($1, $2, 'pending', \
     $3, $4, $5, $6) RETURNING *",
  )
  .bind(input.jobset_id)
  .bind(&input.commit_hash)
  .bind(input.pr_number)
  .bind(&input.pr_head_branch)
  .bind(&input.pr_base_branch)
  .bind(&input.pr_action)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Evaluation for commit '{}' already exists in this jobset",
          input.commit_hash
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>("SELECT * FROM evaluations WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))
}

pub async fn list_for_jobset(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<Vec<Evaluation>> {
  sqlx::query_as::<_, Evaluation>(
    "SELECT * FROM evaluations WHERE jobset_id = $1 ORDER BY evaluation_time \
     DESC",
  )
  .bind(jobset_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// List evaluations with optional `jobset_id` and status filters, with
/// pagination.
pub async fn list_filtered(
  pool: &PgPool,
  jobset_id: Option<Uuid>,
  status: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<Evaluation>> {
  sqlx::query_as::<_, Evaluation>(
    "SELECT * FROM evaluations WHERE ($1::uuid IS NULL OR jobset_id = $1) AND \
     ($2::text IS NULL OR status = $2) ORDER BY evaluation_time DESC LIMIT $3 \
     OFFSET $4",
  )
  .bind(jobset_id)
  .bind(status)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn count_filtered(
  pool: &PgPool,
  jobset_id: Option<Uuid>,
  status: Option<&str>,
) -> Result<i64> {
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM evaluations WHERE ($1::uuid IS NULL OR jobset_id = \
     $1) AND ($2::text IS NULL OR status = $2)",
  )
  .bind(jobset_id)
  .bind(status)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(row.0)
}

pub async fn update_status(
  pool: &PgPool,
  id: Uuid,
  status: EvaluationStatus,
  error_message: Option<&str>,
) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>(
    "UPDATE evaluations SET status = $1, error_message = $2 WHERE id = $3 \
     RETURNING *",
  )
  .bind(status)
  .bind(error_message)
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))
}

pub async fn get_latest(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<Option<Evaluation>> {
  sqlx::query_as::<_, Evaluation>(
    "SELECT * FROM evaluations WHERE jobset_id = $1 ORDER BY evaluation_time \
     DESC LIMIT 1",
  )
  .bind(jobset_id)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)
}

/// Set the inputs hash for an evaluation (used for eval caching).
pub async fn set_inputs_hash(
  pool: &PgPool,
  id: Uuid,
  hash: &str,
) -> Result<()> {
  sqlx::query("UPDATE evaluations SET inputs_hash = $1 WHERE id = $2")
    .bind(hash)
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(())
}

/// Check if an evaluation with the same `inputs_hash` already exists for this
/// jobset.
pub async fn get_by_inputs_hash(
  pool: &PgPool,
  jobset_id: Uuid,
  inputs_hash: &str,
) -> Result<Option<Evaluation>> {
  sqlx::query_as::<_, Evaluation>(
    "SELECT * FROM evaluations WHERE jobset_id = $1 AND inputs_hash = $2 AND \
     status = 'completed' ORDER BY evaluation_time DESC LIMIT 1",
  )
  .bind(jobset_id)
  .bind(inputs_hash)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn count(pool: &PgPool) -> Result<i64> {
  let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(row.0)
}

//! Starred jobs repository - for personalized dashboard

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateStarredJob, StarredJob},
};

/// Create a new starred job
pub async fn create(
  pool: &PgPool,
  user_id: Uuid,
  data: &CreateStarredJob,
) -> Result<StarredJob> {
  sqlx::query_as::<_, StarredJob>(
    "INSERT INTO starred_jobs (user_id, project_id, jobset_id, job_name) \
     VALUES ($1, $2, $3, $4) RETURNING *",
  )
  .bind(user_id)
  .bind(data.project_id)
  .bind(data.jobset_id)
  .bind(&data.job_name)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("Job already starred".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a starred job by ID
pub async fn get(pool: &PgPool, id: Uuid) -> Result<StarredJob> {
  sqlx::query_as::<_, StarredJob>("SELECT * FROM starred_jobs WHERE id = $1")
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
      match e {
        sqlx::Error::RowNotFound => {
          CiError::NotFound(format!("Starred job {id} not found"))
        },
        _ => CiError::Database(e),
      }
    })
}

/// List starred jobs for a user with pagination
pub async fn list_for_user(
  pool: &PgPool,
  user_id: Uuid,
  limit: i64,
  offset: i64,
) -> Result<Vec<StarredJob>> {
  sqlx::query_as::<_, StarredJob>(
    "SELECT * FROM starred_jobs WHERE user_id = $1 ORDER BY created_at DESC \
     LIMIT $2 OFFSET $3",
  )
  .bind(user_id)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Count starred jobs for a user
pub async fn count_for_user(pool: &PgPool, user_id: Uuid) -> Result<i64> {
  let (count,): (i64,) =
    sqlx::query_as("SELECT COUNT(*) FROM starred_jobs WHERE user_id = $1")
      .bind(user_id)
      .fetch_one(pool)
      .await?;
  Ok(count)
}

/// Check if a user has starred a specific job
pub async fn is_starred(
  pool: &PgPool,
  user_id: Uuid,
  project_id: Uuid,
  jobset_id: Option<Uuid>,
  job_name: &str,
) -> Result<bool> {
  let (count,): (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM starred_jobs WHERE user_id = $1 AND project_id = $2 \
     AND jobset_id IS NOT DISTINCT FROM $3 AND job_name = $4",
  )
  .bind(user_id)
  .bind(project_id)
  .bind(jobset_id)
  .bind(job_name)
  .fetch_one(pool)
  .await?;
  Ok(count > 0)
}

/// Delete a starred job
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM starred_jobs WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Starred job {id} not found")));
  }
  Ok(())
}

/// Delete a starred job by user and job details
pub async fn delete_by_job(
  pool: &PgPool,
  user_id: Uuid,
  project_id: Uuid,
  jobset_id: Option<Uuid>,
  job_name: &str,
) -> Result<()> {
  let result = sqlx::query(
    "DELETE FROM starred_jobs WHERE user_id = $1 AND project_id = $2 AND \
     jobset_id IS NOT DISTINCT FROM $3 AND job_name = $4",
  )
  .bind(user_id)
  .bind(project_id)
  .bind(jobset_id)
  .bind(job_name)
  .execute(pool)
  .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound("Starred job not found".to_string()));
  }
  Ok(())
}

/// Delete all starred jobs for a user (when user is deleted)
pub async fn delete_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<()> {
  sqlx::query("DELETE FROM starred_jobs WHERE user_id = $1")
    .bind(user_id)
    .execute(pool)
    .await?;
  Ok(())
}

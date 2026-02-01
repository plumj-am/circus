use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{Build, BuildStats, BuildStatus, CreateBuild},
};

pub async fn create(pool: &PgPool, input: CreateBuild) -> Result<Build> {
  let is_aggregate = input.is_aggregate.unwrap_or(false);
  sqlx::query_as::<_, Build>(
    "INSERT INTO builds (evaluation_id, job_name, drv_path, status, system, \
     outputs, is_aggregate, constituents) VALUES ($1, $2, $3, 'pending', $4, \
     $5, $6, $7) RETURNING *",
  )
  .bind(input.evaluation_id)
  .bind(&input.job_name)
  .bind(&input.drv_path)
  .bind(&input.system)
  .bind(&input.outputs)
  .bind(is_aggregate)
  .bind(&input.constituents)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Build for job '{}' already exists in this evaluation",
          input.job_name
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

pub async fn get_completed_by_drv_path(
  pool: &PgPool,
  drv_path: &str,
) -> Result<Option<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT * FROM builds WHERE drv_path = $1 AND status = 'completed' LIMIT 1",
  )
  .bind(drv_path)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Build> {
  sqlx::query_as::<_, Build>("SELECT * FROM builds WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Build {id} not found")))
}

pub async fn list_for_evaluation(
  pool: &PgPool,
  evaluation_id: Uuid,
) -> Result<Vec<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT * FROM builds WHERE evaluation_id = $1 ORDER BY created_at DESC",
  )
  .bind(evaluation_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn list_pending(pool: &PgPool, limit: i64) -> Result<Vec<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT b.* FROM builds b JOIN evaluations e ON b.evaluation_id = e.id \
     JOIN jobsets j ON e.jobset_id = j.id WHERE b.status = 'pending' ORDER BY \
     b.priority DESC, j.scheduling_shares DESC, b.created_at ASC LIMIT $1",
  )
  .bind(limit)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Atomically claim a pending build by setting it to running.
/// Returns `None` if the build was already claimed by another worker.
pub async fn start(pool: &PgPool, id: Uuid) -> Result<Option<Build>> {
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = 'running', started_at = NOW() WHERE id = $1 \
     AND status = 'pending' RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn complete(
  pool: &PgPool,
  id: Uuid,
  status: BuildStatus,
  log_path: Option<&str>,
  build_output_path: Option<&str>,
  error_message: Option<&str>,
) -> Result<Build> {
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = $1, completed_at = NOW(), log_path = $2, \
     build_output_path = $3, error_message = $4 WHERE id = $5 RETURNING *",
  )
  .bind(status)
  .bind(log_path)
  .bind(build_output_path)
  .bind(error_message)
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Build {id} not found")))
}

pub async fn list_recent(pool: &PgPool, limit: i64) -> Result<Vec<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT * FROM builds ORDER BY created_at DESC LIMIT $1",
  )
  .bind(limit)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT b.* FROM builds b JOIN evaluations e ON b.evaluation_id = e.id \
     JOIN jobsets j ON e.jobset_id = j.id WHERE j.project_id = $1 ORDER BY \
     b.created_at DESC",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn get_stats(pool: &PgPool) -> Result<BuildStats> {
  sqlx::query_as::<_, BuildStats>("SELECT * FROM build_stats")
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
    .map(|opt| opt.unwrap_or_default())
}

/// Reset builds that were left in 'running' state (orphaned by a crashed
/// runner). Limited to 50 builds per call to prevent thundering herd.
pub async fn reset_orphaned(
  pool: &PgPool,
  older_than_secs: i64,
) -> Result<u64> {
  let result = sqlx::query(
    "UPDATE builds SET status = 'pending', started_at = NULL WHERE id IN \
     (SELECT id FROM builds WHERE status = 'running' AND started_at < NOW() - \
     make_interval(secs => $1) LIMIT 50)",
  )
  .bind(older_than_secs)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;

  Ok(result.rows_affected())
}

/// List builds with optional evaluation_id, status, system, and job_name
/// filters, with pagination.
pub async fn list_filtered(
  pool: &PgPool,
  evaluation_id: Option<Uuid>,
  status: Option<&str>,
  system: Option<&str>,
  job_name: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<Build>> {
  sqlx::query_as::<_, Build>(
    "SELECT * FROM builds WHERE ($1::uuid IS NULL OR evaluation_id = $1) AND \
     ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR system = $3) \
     AND ($4::text IS NULL OR job_name ILIKE '%' || $4 || '%') ORDER BY \
     created_at DESC LIMIT $5 OFFSET $6",
  )
  .bind(evaluation_id)
  .bind(status)
  .bind(system)
  .bind(job_name)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn count_filtered(
  pool: &PgPool,
  evaluation_id: Option<Uuid>,
  status: Option<&str>,
  system: Option<&str>,
  job_name: Option<&str>,
) -> Result<i64> {
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM builds WHERE ($1::uuid IS NULL OR evaluation_id = \
     $1) AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR \
     system = $3) AND ($4::text IS NULL OR job_name ILIKE '%' || $4 || '%')",
  )
  .bind(evaluation_id)
  .bind(status)
  .bind(system)
  .bind(job_name)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(row.0)
}

pub async fn cancel(pool: &PgPool, id: Uuid) -> Result<Build> {
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = 'cancelled', completed_at = NOW() WHERE id = \
     $1 AND status IN ('pending', 'running') RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| {
    CiError::NotFound(format!(
      "Build {id} not found or not in a cancellable state"
    ))
  })
}

/// Cancel a build and all its transitive dependents.
pub async fn cancel_cascade(pool: &PgPool, id: Uuid) -> Result<Vec<Build>> {
  let mut cancelled = Vec::new();

  // Cancel the target build
  if let Ok(build) = cancel(pool, id).await {
    cancelled.push(build);
  }

  // Find and cancel all dependents recursively
  let mut to_cancel: Vec<Uuid> = vec![id];
  while let Some(build_id) = to_cancel.pop() {
    let dependents: Vec<(Uuid,)> = sqlx::query_as(
      "SELECT build_id FROM build_dependencies WHERE dependency_build_id = $1",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)?;

    for (dep_id,) in dependents {
      if let Ok(build) = cancel(pool, dep_id).await {
        to_cancel.push(dep_id);
        cancelled.push(build);
      }
    }
  }

  Ok(cancelled)
}

/// Restart a build by resetting it to pending state.
/// Only works for failed, completed, or cancelled builds.
pub async fn restart(pool: &PgPool, id: Uuid) -> Result<Build> {
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = 'pending', started_at = NULL, completed_at = \
     NULL, log_path = NULL, build_output_path = NULL, error_message = NULL, \
     retry_count = retry_count + 1 WHERE id = $1 AND status IN ('failed', \
     'completed', 'cancelled') RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| {
    CiError::NotFound(format!(
      "Build {id} not found or not in a restartable state"
    ))
  })
}

/// Mark a build's outputs as signed.
pub async fn mark_signed(pool: &PgPool, id: Uuid) -> Result<()> {
  sqlx::query("UPDATE builds SET signed = true WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(())
}

/// Batch-fetch completed builds by derivation paths.
/// Returns a map from drv_path to Build for deduplication.
pub async fn get_completed_by_drv_paths(
  pool: &PgPool,
  drv_paths: &[String],
) -> Result<std::collections::HashMap<String, Build>> {
  if drv_paths.is_empty() {
    return Ok(std::collections::HashMap::new());
  }
  let builds = sqlx::query_as::<_, Build>(
    "SELECT DISTINCT ON (drv_path) * FROM builds WHERE drv_path = ANY($1) AND \
     status = 'completed' ORDER BY drv_path, completed_at DESC",
  )
  .bind(drv_paths)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)?;

  Ok(
    builds
      .into_iter()
      .map(|b| (b.drv_path.clone(), b))
      .collect(),
  )
}

/// Set the builder_id for a build.
pub async fn set_builder(
  pool: &PgPool,
  id: Uuid,
  builder_id: Uuid,
) -> Result<()> {
  sqlx::query("UPDATE builds SET builder_id = $1 WHERE id = $2")
    .bind(builder_id)
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(())
}

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::BuildMetric,
};

pub async fn upsert(
  pool: &PgPool,
  build_id: Uuid,
  metric_name: &str,
  metric_value: f64,
  unit: &str,
) -> Result<BuildMetric> {
  sqlx::query_as::<_, BuildMetric>(
    "INSERT INTO build_metrics (build_id, metric_name, metric_value, unit) \
     VALUES ($1, $2, $3, $4) ON CONFLICT (build_id, metric_name) DO UPDATE \
     SET metric_value = EXCLUDED.metric_value, collected_at = NOW() RETURNING \
     *",
  )
  .bind(build_id)
  .bind(metric_name)
  .bind(metric_value)
  .bind(unit)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn calculate_failure_rate(
  pool: &PgPool,
  project_id: Option<Uuid>,
  jobset_id: Option<Uuid>,
  window_minutes: i64,
) -> Result<f64> {
  let rows: Vec<(Uuid, String)> = sqlx::query_as(
    "SELECT b.id, b.status::text FROM builds b JOIN evaluations e ON \
     b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE \
     ($1::uuid IS NULL OR j.project_id = $1) AND ($2::uuid IS NULL OR j.id = \
     $2) AND b.completed_at > NOW() - (INTERVAL '1 minute' * $3) ORDER BY \
     b.completed_at DESC",
  )
  .bind(project_id)
  .bind(jobset_id)
  .bind(window_minutes)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)?;

  if rows.is_empty() {
    return Ok(0.0);
  }

  let failed_count = rows
    .iter()
    .filter(|(_, status)| *status == "Failed")
    .count();
  Ok((failed_count as f64) / (rows.len() as f64) * 100.0)
}

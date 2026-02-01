use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateNotificationConfig, NotificationConfig},
};

pub async fn create(
  pool: &PgPool,
  input: CreateNotificationConfig,
) -> Result<NotificationConfig> {
  sqlx::query_as::<_, NotificationConfig>(
    "INSERT INTO notification_configs (project_id, notification_type, config) \
     VALUES ($1, $2, $3) RETURNING *",
  )
  .bind(input.project_id)
  .bind(&input.notification_type)
  .bind(&input.config)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Notification config '{}' already exists for this project",
          input.notification_type
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<NotificationConfig>> {
  sqlx::query_as::<_, NotificationConfig>(
    "SELECT * FROM notification_configs WHERE project_id = $1 AND enabled = \
     true ORDER BY created_at DESC",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM notification_configs WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!(
      "Notification config {id} not found"
    )));
  }
  Ok(())
}

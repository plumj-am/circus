use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  config::DeclarativeWebhook,
  error::{CiError, Result},
  models::{CreateWebhookConfig, WebhookConfig},
};

/// Create a new webhook config.
///
/// # Errors
///
/// Returns error if database insert fails or config already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateWebhookConfig,
  secret_hash: Option<&str>,
) -> Result<WebhookConfig> {
  sqlx::query_as::<_, WebhookConfig>(
    "INSERT INTO webhook_configs (project_id, forge_type, secret_hash) VALUES \
     ($1, $2, $3) RETURNING *",
  )
  .bind(input.project_id)
  .bind(&input.forge_type)
  .bind(secret_hash)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Webhook config for forge '{}' already exists for this project",
          input.forge_type
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a webhook config by ID.
///
/// # Errors
///
/// Returns error if database query fails or config not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<WebhookConfig> {
  sqlx::query_as::<_, WebhookConfig>(
    "SELECT * FROM webhook_configs WHERE id = $1",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Webhook config {id} not found")))
}

/// List all webhook configs for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<WebhookConfig>> {
  sqlx::query_as::<_, WebhookConfig>(
    "SELECT * FROM webhook_configs WHERE project_id = $1 ORDER BY created_at \
     DESC",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Get a webhook config by project and forge type.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_project_and_forge(
  pool: &PgPool,
  project_id: Uuid,
  forge_type: &str,
) -> Result<Option<WebhookConfig>> {
  sqlx::query_as::<_, WebhookConfig>(
    "SELECT * FROM webhook_configs WHERE project_id = $1 AND forge_type = $2 \
     AND enabled = true",
  )
  .bind(project_id)
  .bind(forge_type)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)
}

/// Delete a webhook config.
///
/// # Errors
///
/// Returns error if database delete fails or config not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM webhook_configs WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Webhook config {id} not found")));
  }
  Ok(())
}

/// Upsert a webhook config (insert or update on conflict).
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  project_id: Uuid,
  forge_type: &str,
  secret_hash: Option<&str>,
  enabled: bool,
) -> Result<WebhookConfig> {
  sqlx::query_as::<_, WebhookConfig>(
    "INSERT INTO webhook_configs (project_id, forge_type, secret_hash, \
     enabled) VALUES ($1, $2, $3, $4) ON CONFLICT (project_id, forge_type) DO \
     UPDATE SET secret_hash = COALESCE(EXCLUDED.secret_hash, \
     webhook_configs.secret_hash), enabled = EXCLUDED.enabled RETURNING *",
  )
  .bind(project_id)
  .bind(forge_type)
  .bind(secret_hash)
  .bind(enabled)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
}

/// Sync webhook configs from declarative config.
/// Deletes configs not in the declarative list and upserts those that are.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn sync_for_project(
  pool: &PgPool,
  project_id: Uuid,
  webhooks: &[DeclarativeWebhook],
  resolve_secret: impl Fn(&DeclarativeWebhook) -> Option<String>,
) -> Result<()> {
  // Get forge types from declarative config
  let types: Vec<&str> =
    webhooks.iter().map(|w| w.forge_type.as_str()).collect();

  // Delete webhook configs not in declarative config
  sqlx::query(
    "DELETE FROM webhook_configs WHERE project_id = $1 AND forge_type != \
     ALL($2::text[])",
  )
  .bind(project_id)
  .bind(&types)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;

  // Upsert each webhook config
  for webhook in webhooks {
    let secret = resolve_secret(webhook);

    upsert(
      pool,
      project_id,
      &webhook.forge_type,
      secret.as_deref(),
      webhook.enabled,
    )
    .await?;
  }

  Ok(())
}

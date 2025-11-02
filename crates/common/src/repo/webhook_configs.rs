use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::{CreateWebhookConfig, WebhookConfig};

pub async fn create(
    pool: &PgPool,
    input: CreateWebhookConfig,
    secret_hash: Option<&str>,
) -> Result<WebhookConfig> {
    sqlx::query_as::<_, WebhookConfig>(
        "INSERT INTO webhook_configs (project_id, forge_type, secret_hash) VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(input.project_id)
    .bind(&input.forge_type)
    .bind(secret_hash)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            CiError::Conflict(format!(
                "Webhook config for forge '{}' already exists for this project",
                input.forge_type
            ))
        }
        _ => CiError::Database(e),
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<WebhookConfig> {
    sqlx::query_as::<_, WebhookConfig>("SELECT * FROM webhook_configs WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CiError::NotFound(format!("Webhook config {id} not found")))
}

pub async fn list_for_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<WebhookConfig>> {
    sqlx::query_as::<_, WebhookConfig>(
        "SELECT * FROM webhook_configs WHERE project_id = $1 ORDER BY created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn get_by_project_and_forge(
    pool: &PgPool,
    project_id: Uuid,
    forge_type: &str,
) -> Result<Option<WebhookConfig>> {
    sqlx::query_as::<_, WebhookConfig>(
        "SELECT * FROM webhook_configs WHERE project_id = $1 AND forge_type = $2 AND enabled = true",
    )
    .bind(project_id)
    .bind(forge_type)
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
}

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

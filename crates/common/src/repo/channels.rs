use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::{Channel, CreateChannel};

pub async fn create(pool: &PgPool, input: CreateChannel) -> Result<Channel> {
    sqlx::query_as::<_, Channel>(
        "INSERT INTO channels (project_id, name, jobset_id) \
         VALUES ($1, $2, $3) RETURNING *",
    )
    .bind(input.project_id)
    .bind(&input.name)
    .bind(input.jobset_id)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => CiError::Conflict(
            format!("Channel '{}' already exists for this project", input.name),
        ),
        _ => CiError::Database(e),
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<Channel> {
    sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CiError::NotFound(format!("Channel {id} not found")))
}

pub async fn list_for_project(pool: &PgPool, project_id: Uuid) -> Result<Vec<Channel>> {
    sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE project_id = $1 ORDER BY name")
        .bind(project_id)
        .fetch_all(pool)
        .await
        .map_err(CiError::Database)
}

pub async fn list_all(pool: &PgPool) -> Result<Vec<Channel>> {
    sqlx::query_as::<_, Channel>("SELECT * FROM channels ORDER BY name")
        .fetch_all(pool)
        .await
        .map_err(CiError::Database)
}

/// Promote an evaluation to a channel (set it as the current evaluation).
pub async fn promote(pool: &PgPool, channel_id: Uuid, evaluation_id: Uuid) -> Result<Channel> {
    sqlx::query_as::<_, Channel>(
        "UPDATE channels SET current_evaluation_id = $1, updated_at = NOW() \
         WHERE id = $2 RETURNING *",
    )
    .bind(evaluation_id)
    .bind(channel_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Channel {channel_id} not found")))
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query("DELETE FROM channels WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(CiError::Database)?;
    if result.rows_affected() == 0 {
        return Err(CiError::NotFound(format!("Channel {id} not found")));
    }
    Ok(())
}

/// Find the channel for a jobset and auto-promote if all builds in the evaluation succeeded.
pub async fn auto_promote_if_complete(
    pool: &PgPool,
    jobset_id: Uuid,
    evaluation_id: Uuid,
) -> Result<()> {
    // Check if all builds for this evaluation are completed
    let row: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'completed') \
         FROM builds WHERE evaluation_id = $1",
    )
    .bind(evaluation_id)
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)?;

    let (total, completed) = row;
    if total == 0 || total != completed {
        return Ok(());
    }

    // All builds completed — promote to any channels tracking this jobset
    let channels = sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE jobset_id = $1")
        .bind(jobset_id)
        .fetch_all(pool)
        .await
        .map_err(CiError::Database)?;

    for channel in channels {
        let _ = promote(pool, channel.id, evaluation_id).await;
        tracing::info!(
            channel = %channel.name,
            evaluation_id = %evaluation_id,
            "Auto-promoted evaluation to channel"
        );
    }

    Ok(())
}

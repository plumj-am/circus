use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::{CreateRemoteBuilder, RemoteBuilder};

pub async fn create(pool: &PgPool, input: CreateRemoteBuilder) -> Result<RemoteBuilder> {
    sqlx::query_as::<_, RemoteBuilder>(
        "INSERT INTO remote_builders (name, ssh_uri, systems, max_jobs, speed_factor, \
         supported_features, mandatory_features, public_host_key, ssh_key_file) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *",
    )
    .bind(&input.name)
    .bind(&input.ssh_uri)
    .bind(&input.systems)
    .bind(input.max_jobs.unwrap_or(1))
    .bind(input.speed_factor.unwrap_or(1))
    .bind(input.supported_features.as_deref().unwrap_or(&[]))
    .bind(input.mandatory_features.as_deref().unwrap_or(&[]))
    .bind(&input.public_host_key)
    .bind(&input.ssh_key_file)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            CiError::Conflict(format!("Remote builder '{}' already exists", input.name))
        }
        _ => CiError::Database(e),
    })
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<RemoteBuilder> {
    sqlx::query_as::<_, RemoteBuilder>("SELECT * FROM remote_builders WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

pub async fn list(pool: &PgPool) -> Result<Vec<RemoteBuilder>> {
    sqlx::query_as::<_, RemoteBuilder>(
        "SELECT * FROM remote_builders ORDER BY speed_factor DESC, name",
    )
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn list_enabled(pool: &PgPool) -> Result<Vec<RemoteBuilder>> {
    sqlx::query_as::<_, RemoteBuilder>(
        "SELECT * FROM remote_builders WHERE enabled = true ORDER BY speed_factor DESC, name",
    )
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

/// Find a suitable builder for the given system.
pub async fn find_for_system(pool: &PgPool, system: &str) -> Result<Vec<RemoteBuilder>> {
    sqlx::query_as::<_, RemoteBuilder>(
        "SELECT * FROM remote_builders WHERE enabled = true AND $1 = ANY(systems) \
         ORDER BY speed_factor DESC",
    )
    .bind(system)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    input: crate::models::UpdateRemoteBuilder,
) -> Result<RemoteBuilder> {
    // Build dynamic update — use COALESCE pattern
    sqlx::query_as::<_, RemoteBuilder>(
        "UPDATE remote_builders SET \
         name = COALESCE($1, name), \
         ssh_uri = COALESCE($2, ssh_uri), \
         systems = COALESCE($3, systems), \
         max_jobs = COALESCE($4, max_jobs), \
         speed_factor = COALESCE($5, speed_factor), \
         supported_features = COALESCE($6, supported_features), \
         mandatory_features = COALESCE($7, mandatory_features), \
         enabled = COALESCE($8, enabled), \
         public_host_key = COALESCE($9, public_host_key), \
         ssh_key_file = COALESCE($10, ssh_key_file) \
         WHERE id = $11 RETURNING *",
    )
    .bind(&input.name)
    .bind(&input.ssh_uri)
    .bind(&input.systems)
    .bind(input.max_jobs)
    .bind(input.speed_factor)
    .bind(&input.supported_features)
    .bind(&input.mandatory_features)
    .bind(input.enabled)
    .bind(&input.public_host_key)
    .bind(&input.ssh_key_file)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
    let result = sqlx::query("DELETE FROM remote_builders WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(CiError::Database)?;
    if result.rows_affected() == 0 {
        return Err(CiError::NotFound(format!("Remote builder {id} not found")));
    }
    Ok(())
}

pub async fn count(pool: &PgPool) -> Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM remote_builders")
        .fetch_one(pool)
        .await
        .map_err(CiError::Database)?;
    Ok(row.0)
}

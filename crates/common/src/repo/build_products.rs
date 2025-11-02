use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};
use crate::models::{BuildProduct, CreateBuildProduct};

pub async fn create(pool: &PgPool, input: CreateBuildProduct) -> Result<BuildProduct> {
    sqlx::query_as::<_, BuildProduct>(
        "INSERT INTO build_products (build_id, name, path, sha256_hash, file_size, content_type, is_directory) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(input.build_id)
    .bind(&input.name)
    .bind(&input.path)
    .bind(&input.sha256_hash)
    .bind(input.file_size)
    .bind(&input.content_type)
    .bind(input.is_directory)
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<BuildProduct> {
    sqlx::query_as::<_, BuildProduct>("SELECT * FROM build_products WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| CiError::NotFound(format!("Build product {id} not found")))
}

pub async fn list_for_build(pool: &PgPool, build_id: Uuid) -> Result<Vec<BuildProduct>> {
    sqlx::query_as::<_, BuildProduct>(
        "SELECT * FROM build_products WHERE build_id = $1 ORDER BY created_at ASC",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::BuildDependency,
};

pub async fn create(
  pool: &PgPool,
  build_id: Uuid,
  dependency_build_id: Uuid,
) -> Result<BuildDependency> {
  sqlx::query_as::<_, BuildDependency>(
    "INSERT INTO build_dependencies (build_id, dependency_build_id) VALUES \
     ($1, $2) RETURNING *",
  )
  .bind(build_id)
  .bind(dependency_build_id)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Dependency from {build_id} to {dependency_build_id} already exists"
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

pub async fn list_for_build(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<BuildDependency>> {
  sqlx::query_as::<_, BuildDependency>(
    "SELECT * FROM build_dependencies WHERE build_id = $1",
  )
  .bind(build_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Batch check if all dependency builds are completed for multiple builds at
/// once. Returns a map from build_id to whether all deps are completed.
pub async fn check_deps_for_builds(
  pool: &PgPool,
  build_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, bool>> {
  if build_ids.is_empty() {
    return Ok(std::collections::HashMap::new());
  }

  // Find build_ids that have incomplete deps
  let rows: Vec<(Uuid,)> = sqlx::query_as(
    "SELECT DISTINCT bd.build_id FROM build_dependencies bd JOIN builds b ON \
     bd.dependency_build_id = b.id WHERE bd.build_id = ANY($1) AND b.status \
     != 'completed'",
  )
  .bind(build_ids)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)?;

  let incomplete: std::collections::HashSet<Uuid> =
    rows.into_iter().map(|(id,)| id).collect();

  Ok(
    build_ids
      .iter()
      .map(|id| (*id, !incomplete.contains(id)))
      .collect(),
  )
}

/// Check if all dependency builds for a given build are completed.
pub async fn all_deps_completed(pool: &PgPool, build_id: Uuid) -> Result<bool> {
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM build_dependencies bd JOIN builds b ON \
     bd.dependency_build_id = b.id WHERE bd.build_id = $1 AND b.status != \
     'completed'",
  )
  .bind(build_id)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;

  Ok(row.0 == 0)
}

//! Project members repository - for per-project permissions

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateProjectMember, ProjectMember, UpdateProjectMember},
  roles::{VALID_PROJECT_ROLES, has_project_permission},
  validation::validate_role,
};

/// Add a member to a project with role validation
pub async fn create(
  pool: &PgPool,
  project_id: Uuid,
  data: &CreateProjectMember,
) -> Result<ProjectMember> {
  // Validate role
  validate_role(&data.role, VALID_PROJECT_ROLES)
    .map_err(|e| CiError::Validation(e.to_string()))?;

  sqlx::query_as::<_, ProjectMember>(
    "INSERT INTO project_members (project_id, user_id, role) VALUES ($1, $2, \
     $3) RETURNING *",
  )
  .bind(project_id)
  .bind(data.user_id)
  .bind(&data.role)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(
          "User is already a member of this project".to_string(),
        )
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a project member by ID
pub async fn get(pool: &PgPool, id: Uuid) -> Result<ProjectMember> {
  sqlx::query_as::<_, ProjectMember>(
    "SELECT * FROM project_members WHERE id = $1",
  )
  .bind(id)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match e {
      sqlx::Error::RowNotFound => {
        CiError::NotFound(format!("Project member {} not found", id))
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a project member by project and user
pub async fn get_by_project_and_user(
  pool: &PgPool,
  project_id: Uuid,
  user_id: Uuid,
) -> Result<Option<ProjectMember>> {
  sqlx::query_as::<_, ProjectMember>(
    "SELECT * FROM project_members WHERE project_id = $1 AND user_id = $2",
  )
  .bind(project_id)
  .bind(user_id)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)
}

/// List all members of a project
pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<ProjectMember>> {
  sqlx::query_as::<_, ProjectMember>(
    "SELECT * FROM project_members WHERE project_id = $1 ORDER BY created_at",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// List all projects a user is a member of
pub async fn list_for_user(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<ProjectMember>> {
  sqlx::query_as::<_, ProjectMember>(
    "SELECT * FROM project_members WHERE user_id = $1 ORDER BY created_at",
  )
  .bind(user_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Update a project member's role with validation
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  data: &UpdateProjectMember,
) -> Result<ProjectMember> {
  if let Some(ref role) = data.role {
    validate_role(role, VALID_PROJECT_ROLES)
      .map_err(|e| CiError::Validation(e.to_string()))?;

    sqlx::query_as::<_, ProjectMember>(
      "UPDATE project_members SET role = $1 WHERE id = $2 RETURNING *",
    )
    .bind(role)
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| {
      match e {
        sqlx::Error::RowNotFound => {
          CiError::NotFound(format!("Project member {} not found", id))
        },
        _ => CiError::Database(e),
      }
    })
  } else {
    get(pool, id).await
  }
}

/// Remove a member from a project
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM project_members WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!(
      "Project member {} not found",
      id
    )));
  }
  Ok(())
}

/// Remove a specific user from a project
pub async fn delete_by_project_and_user(
  pool: &PgPool,
  project_id: Uuid,
  user_id: Uuid,
) -> Result<()> {
  let result = sqlx::query(
    "DELETE FROM project_members WHERE project_id = $1 AND user_id = $2",
  )
  .bind(project_id)
  .bind(user_id)
  .execute(pool)
  .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(
      "User is not a member of this project".to_string(),
    ));
  }
  Ok(())
}

/// Check if a user has a specific role or higher in a project
pub async fn check_permission(
  pool: &PgPool,
  project_id: Uuid,
  user_id: Uuid,
  required_role: &str,
) -> Result<bool> {
  use crate::roles::has_project_permission;

  let member = get_by_project_and_user(pool, project_id, user_id).await?;

  if let Some(m) = member {
    Ok(has_project_permission(&m.role, required_role))
  } else {
    Ok(false)
  }
}

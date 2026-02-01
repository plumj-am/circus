use axum::{
  Json,
  Router,
  extract::{Path, State},
  routing::get,
};
use fc_common::{
  Validate,
  models::{
    CreateRemoteBuilder,
    RemoteBuilder,
    SystemStatus,
    UpdateRemoteBuilder,
  },
};
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

async fn list_builders(
  State(state): State<AppState>,
) -> Result<Json<Vec<RemoteBuilder>>, ApiError> {
  let builders = fc_common::repo::remote_builders::list(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(builders))
}

async fn get_builder(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  let builder = fc_common::repo::remote_builders::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(builder))
}

async fn create_builder(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Json(input): Json<CreateRemoteBuilder>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(fc_common::CiError::Validation(msg)))?;
  let builder = fc_common::repo::remote_builders::create(&state.pool, input)
    .await
    .map_err(ApiError)?;
  Ok(Json(builder))
}

async fn update_builder(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Json(input): Json<UpdateRemoteBuilder>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(fc_common::CiError::Validation(msg)))?;
  let builder =
    fc_common::repo::remote_builders::update(&state.pool, id, input)
      .await
      .map_err(ApiError)?;
  Ok(Json(builder))
}

async fn delete_builder(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  fc_common::repo::remote_builders::delete(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(serde_json::json!({"deleted": true})))
}

async fn system_status(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<SystemStatus>, ApiError> {
  let pool = &state.pool;

  let projects: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(fc_common::CiError::Database(e)))?;
  let jobsets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(fc_common::CiError::Database(e)))?;
  let evaluations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(fc_common::CiError::Database(e)))?;

  let stats = fc_common::repo::builds::get_stats(pool)
    .await
    .map_err(ApiError)?;
  let builders = fc_common::repo::remote_builders::count(pool)
    .await
    .map_err(ApiError)?;

  let channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(fc_common::CiError::Database(e)))?;

  Ok(Json(SystemStatus {
    projects_count:    projects.0,
    jobsets_count:     jobsets.0,
    evaluations_count: evaluations.0,
    builds_pending:    stats.pending_builds.unwrap_or(0),
    builds_running:    stats.running_builds.unwrap_or(0),
    builds_completed:  stats.completed_builds.unwrap_or(0),
    builds_failed:     stats.failed_builds.unwrap_or(0),
    remote_builders:   builders,
    channels_count:    channels.0,
  }))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/admin/builders", get(list_builders).post(create_builder))
    .route(
      "/admin/builders/{id}",
      get(get_builder).put(update_builder).delete(delete_builder),
    )
    .route("/admin/system", get(system_status))
}

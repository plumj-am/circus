use axum::{
  Json,
  Router,
  extract::{Path, State},
  routing::{get, post},
};
use circus_common::{
  Validate,
  models::{
    CreateRemoteBuilder,
    NotificationTask,
    RemoteBuilder,
    SystemStatus,
    UpdateRemoteBuilder,
  },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

fn config_file_path() -> std::path::PathBuf {
  std::env::var_os("CIRCUS_CONFIG_FILE")
    .map(std::path::PathBuf::from)
    .unwrap_or_else(|| std::path::PathBuf::from("circus.toml"))
}

async fn list_builders(
  State(state): State<AppState>,
) -> Result<Json<Vec<RemoteBuilder>>, ApiError> {
  let builders = circus_common::repo::remote_builders::list(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(builders))
}

async fn get_builder(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  let builder = circus_common::repo::remote_builders::get(&state.pool, id)
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
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let builder =
    circus_common::repo::remote_builders::create(&state.pool, input)
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
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let builder =
    circus_common::repo::remote_builders::update(&state.pool, id, input)
      .await
      .map_err(ApiError)?;
  Ok(Json(builder))
}

async fn delete_builder(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::remote_builders::delete(&state.pool, id)
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
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;
  let jobsets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;
  let evaluations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  let build_stats = circus_common::repo::builds::get_stats(pool)
    .await
    .map_err(ApiError)?;
  let builders = circus_common::repo::remote_builders::count(pool)
    .await
    .map_err(ApiError)?;

  let channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  Ok(Json(SystemStatus {
    projects_count:    projects.0,
    jobsets_count:     jobsets.0,
    evaluations_count: evaluations.0,
    builds_pending:    build_stats.pending_builds.unwrap_or(0),
    builds_running:    build_stats.running_builds.unwrap_or(0),
    builds_completed:  build_stats.completed_builds.unwrap_or(0),
    builds_failed:     build_stats.failed_builds.unwrap_or(0),
    remote_builders:   builders,
    channels_count:    channels.0,
  }))
}

async fn list_notification_tasks(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<Vec<NotificationTask>>, ApiError> {
  let tasks =
    circus_common::repo::notification_tasks::list_recent(&state.pool, 100)
      .await
      .map_err(ApiError)?;
  Ok(Json(tasks))
}

async fn retry_notification_task(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<NotificationTask>, ApiError> {
  let task =
    circus_common::repo::notification_tasks::requeue_failed(&state.pool, id)
      .await
      .map_err(ApiError)?;
  Ok(Json(task))
}

#[derive(Debug, Serialize)]
struct ConfigFileResponse {
  path:             String,
  contents:         String,
  requires_restart: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateConfigFile {
  contents: String,
}

async fn get_config_file(
  _auth: RequireAdmin,
) -> Result<Json<ConfigFileResponse>, ApiError> {
  let path = config_file_path();
  let contents = match tokio::fs::read_to_string(&path).await {
    Ok(contents) => contents,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
      toml::to_string_pretty(&circus_common::config::Config::default())
        .map_err(|e| {
          ApiError(circus_common::CiError::Internal(format!(
            "Failed to render default configuration: {e}"
          )))
        })?
    },
    Err(e) => return Err(ApiError(circus_common::CiError::Io(e))),
  };

  Ok(Json(ConfigFileResponse {
    path: path.display().to_string(),
    contents,
    requires_restart: true,
  }))
}

async fn update_config_file(
  _auth: RequireAdmin,
  Json(input): Json<UpdateConfigFile>,
) -> Result<Json<ConfigFileResponse>, ApiError> {
  let parsed: circus_common::config::Config = toml::from_str(&input.contents)
    .map_err(|e| {
    ApiError(circus_common::CiError::Validation(format!(
      "Invalid TOML configuration: {e}"
    )))
  })?;
  parsed
    .validate()
    .map_err(|e| ApiError(circus_common::CiError::Validation(e.to_string())))?;

  let path = config_file_path();
  let tmp_path = path.with_extension("toml.tmp");
  tokio::fs::write(&tmp_path, &input.contents)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;
  tokio::fs::rename(&tmp_path, &path)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

  Ok(Json(ConfigFileResponse {
    path:             path.display().to_string(),
    contents:         input.contents,
    requires_restart: true,
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
    .route("/admin/notification-tasks", get(list_notification_tasks))
    .route(
      "/admin/notification-tasks/{id}/retry",
      post(retry_notification_task),
    )
    .route(
      "/admin/config",
      get(get_config_file).put(update_config_file),
    )
}

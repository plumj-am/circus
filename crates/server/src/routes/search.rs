use axum::{
  Json,
  Router,
  extract::{Query, State},
  routing::get,
};
use fc_common::models::{Build, Project};
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
struct SearchParams {
  q: String,
}

#[derive(Debug, Serialize)]
struct SearchResults {
  projects: Vec<Project>,
  builds:   Vec<Build>,
}

async fn search(
  State(state): State<AppState>,
  Query(params): Query<SearchParams>,
) -> Result<Json<SearchResults>, ApiError> {
  let query = params.q.trim();
  if query.is_empty() || query.len() > 256 {
    return Ok(Json(SearchResults {
      projects: vec![],
      builds:   vec![],
    }));
  }

  let pattern = format!("%{query}%");

  let projects = sqlx::query_as::<_, Project>(
    "SELECT * FROM projects WHERE name ILIKE $1 OR description ILIKE $1 ORDER \
     BY name LIMIT 20",
  )
  .bind(&pattern)
  .fetch_all(&state.pool)
  .await
  .map_err(|e| ApiError(fc_common::CiError::Database(e)))?;

  let builds = sqlx::query_as::<_, Build>(
    "SELECT * FROM builds WHERE job_name ILIKE $1 OR drv_path ILIKE $1 ORDER \
     BY created_at DESC LIMIT 20",
  )
  .bind(&pattern)
  .fetch_all(&state.pool)
  .await
  .map_err(|e| ApiError(fc_common::CiError::Database(e)))?;

  Ok(Json(SearchResults { projects, builds }))
}

pub fn router() -> Router<AppState> {
  Router::new().route("/search", get(search))
}

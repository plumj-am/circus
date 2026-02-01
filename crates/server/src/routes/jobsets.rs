use axum::{
    Json, Router,
    extract::{Path, State},
    routing::get,
};
use fc_common::{Jobset, UpdateJobset, Validate};
use uuid::Uuid;

use crate::auth_middleware::RequireAdmin;
use crate::error::ApiError;
use crate::state::AppState;

async fn get_jobset(
    State(state): State<AppState>,
    Path((_project_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Jobset>, ApiError> {
    let jobset = fc_common::repo::jobsets::get(&state.pool, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(jobset))
}

async fn update_jobset(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path((_project_id, id)): Path<(Uuid, Uuid)>,
    Json(input): Json<UpdateJobset>,
) -> Result<Json<Jobset>, ApiError> {
    input
        .validate()
        .map_err(|msg| ApiError(fc_common::CiError::Validation(msg)))?;
    let jobset = fc_common::repo::jobsets::update(&state.pool, id, input)
        .await
        .map_err(ApiError)?;
    Ok(Json(jobset))
}

async fn delete_jobset(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path((_project_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    fc_common::repo::jobsets::delete(&state.pool, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/projects/{project_id}/jobsets/{id}",
        get(get_jobset).put(update_jobset).delete(delete_jobset),
    )
}

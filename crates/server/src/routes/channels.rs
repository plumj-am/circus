use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use fc_common::Validate;
use fc_common::models::{Channel, CreateChannel};
use uuid::Uuid;

use crate::auth_middleware::RequireAdmin;
use crate::error::ApiError;
use crate::state::AppState;

async fn list_channels(State(state): State<AppState>) -> Result<Json<Vec<Channel>>, ApiError> {
    let channels = fc_common::repo::channels::list_all(&state.pool)
        .await
        .map_err(ApiError)?;
    Ok(Json(channels))
}

async fn list_project_channels(
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<Channel>>, ApiError> {
    let channels = fc_common::repo::channels::list_for_project(&state.pool, project_id)
        .await
        .map_err(ApiError)?;
    Ok(Json(channels))
}

async fn get_channel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Channel>, ApiError> {
    let channel = fc_common::repo::channels::get(&state.pool, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(channel))
}

async fn create_channel(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Json(input): Json<CreateChannel>,
) -> Result<Json<Channel>, ApiError> {
    input
        .validate()
        .map_err(|msg| ApiError(fc_common::CiError::Validation(msg)))?;
    let channel = fc_common::repo::channels::create(&state.pool, input)
        .await
        .map_err(ApiError)?;
    Ok(Json(channel))
}

async fn delete_channel(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
    fc_common::repo::channels::delete(&state.pool, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(serde_json::json!({"deleted": true})))
}

async fn promote_channel(
    _auth: RequireAdmin,
    State(state): State<AppState>,
    Path((channel_id, eval_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Channel>, ApiError> {
    let channel = fc_common::repo::channels::promote(&state.pool, channel_id, eval_id)
        .await
        .map_err(ApiError)?;
    Ok(Json(channel))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/channels", get(list_channels).post(create_channel))
        .route("/channels/{id}", get(get_channel).delete(delete_channel))
        .route(
            "/channels/{channel_id}/promote/{eval_id}",
            post(promote_channel),
        )
        .route(
            "/projects/{project_id}/channels",
            get(list_project_channels),
        )
}

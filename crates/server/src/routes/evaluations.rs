use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::Extensions,
    routing::{get, post},
};
use fc_common::{CreateEvaluation, Evaluation, PaginatedResponse, PaginationParams, Validate};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth_middleware::RequireRoles;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct ListEvaluationsParams {
    jobset_id: Option<Uuid>,
    status: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn list_evaluations(
    State(state): State<AppState>,
    Query(params): Query<ListEvaluationsParams>,
) -> Result<Json<PaginatedResponse<Evaluation>>, ApiError> {
    let pagination = PaginationParams {
        limit: params.limit,
        offset: params.offset,
    };
    let limit = pagination.limit();
    let offset = pagination.offset();
    let items = fc_common::repo::evaluations::list_filtered(
        &state.pool,
        params.jobset_id,
        params.status.as_deref(),
        limit,
        offset,
    )
    .await
    .map_err(ApiError)?;
    let total = fc_common::repo::evaluations::count_filtered(
        &state.pool,
        params.jobset_id,
        params.status.as_deref(),
    )
    .await
    .map_err(ApiError)?;
    Ok(Json(PaginatedResponse {
        items,
        total,
        limit,
        offset,
    }))
}

async fn get_evaluation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Evaluation>, ApiError> {
    let evaluation = fc_common::repo::evaluations::get(&state.pool, id)
        .await
        .map_err(ApiError)?;
    Ok(Json(evaluation))
}

async fn trigger_evaluation(
    extensions: Extensions,
    State(state): State<AppState>,
    Json(input): Json<CreateEvaluation>,
) -> Result<Json<Evaluation>, ApiError> {
    RequireRoles::check(&extensions, &["eval-jobset"]).map_err(|s| {
        ApiError(if s == axum::http::StatusCode::FORBIDDEN {
            fc_common::CiError::Forbidden("Insufficient permissions".to_string())
        } else {
            fc_common::CiError::Unauthorized("Authentication required".to_string())
        })
    })?;
    input
        .validate()
        .map_err(|msg| ApiError(fc_common::CiError::Validation(msg)))?;
    let evaluation = fc_common::repo::evaluations::create(&state.pool, input)
        .await
        .map_err(ApiError)?;
    Ok(Json(evaluation))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/evaluations", get(list_evaluations))
        .route("/evaluations/{id}", get(get_evaluation))
        .route("/evaluations/trigger", post(trigger_evaluation))
}

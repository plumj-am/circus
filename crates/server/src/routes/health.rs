use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    database: bool,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();

    let status = if db_ok { "ok" } else { "degraded" };

    Json(HealthResponse {
        status,
        database: db_ok,
    })
}

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(health_check))
}

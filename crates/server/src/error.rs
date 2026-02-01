use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fc_common::CiError;
use serde_json::json;

pub struct ApiError(pub CiError);

impl From<CiError> for ApiError {
    fn from(err: CiError) -> Self {
        ApiError(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self.0 {
            CiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            CiError::Validation(msg) => (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.clone()),
            CiError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.clone()),
            CiError::Timeout(msg) => (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", msg.clone()),
            CiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg.clone()),
            CiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone()),
            CiError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DATABASE_ERROR",
                "Internal database error".to_string(),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                "Internal server error".to_string(),
            ),
        };

        let body = axum::Json(json!({ "error": message, "error_code": code }));
        (status, body).into_response()
    }
}

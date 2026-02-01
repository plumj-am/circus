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
      CiError::NotFound(msg) => {
        (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone())
      },
      CiError::Validation(msg) => {
        (StatusCode::BAD_REQUEST, "VALIDATION_ERROR", msg.clone())
      },
      CiError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg.clone()),
      CiError::Timeout(msg) => {
        (StatusCode::REQUEST_TIMEOUT, "TIMEOUT", msg.clone())
      },
      CiError::Unauthorized(msg) => {
        (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg.clone())
      },
      CiError::Forbidden(msg) => {
        (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone())
      },
      CiError::NixEval(msg) => {
        (
          StatusCode::UNPROCESSABLE_ENTITY,
          "NIX_EVAL_ERROR",
          msg.clone(),
        )
      },
      CiError::Build(msg) => {
        (StatusCode::UNPROCESSABLE_ENTITY, "BUILD_ERROR", msg.clone())
      },
      CiError::Config(msg) => {
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "CONFIG_ERROR",
          msg.clone(),
        )
      },
      CiError::Database(e) => {
        tracing::error!(error = %e, "Database error in API handler");
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "DATABASE_ERROR",
          "Internal database error".to_string(),
        )
      },
      CiError::Git(e) => {
        tracing::error!(error = %e, "Git error in API handler");
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "GIT_ERROR",
          format!("Git operation failed: {e}"),
        )
      },
      CiError::Serialization(e) => {
        tracing::error!(error = %e, "Serialization error in API handler");
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "SERIALIZATION_ERROR",
          format!("Data serialization error: {e}"),
        )
      },
      CiError::Io(e) => {
        tracing::error!(error = %e, "IO error in API handler");
        (
          StatusCode::INTERNAL_SERVER_ERROR,
          "IO_ERROR",
          format!("IO error: {e}"),
        )
      },
    };

    if status.is_server_error() {
      tracing::warn!(
          status = %status,
          code = code,
          "API error response: {}",
          message
      );
    }

    let body = axum::Json(json!({ "error": message, "error_code": code }));
    (status, body).into_response()
  }
}

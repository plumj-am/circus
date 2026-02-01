use axum::{
  Json,
  Router,
  body::Bytes,
  extract::{Path, State},
  http::{HeaderMap, StatusCode},
  routing::post,
};
use fc_common::{models::CreateEvaluation, repo};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Serialize)]
struct WebhookResponse {
  accepted: bool,
  message:  String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubPushPayload {
  #[serde(alias = "ref")]
  git_ref:    Option<String>,
  after:      Option<String>,
  repository: Option<GithubRepo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubRepo {
  clone_url: Option<String>,
  html_url:  Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GiteaPushPayload {
  #[serde(alias = "ref")]
  git_ref:    Option<String>,
  after:      Option<String>,
  repository: Option<GiteaRepo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GiteaRepo {
  clone_url: Option<String>,
  html_url:  Option<String>,
}

/// Verify HMAC-SHA256 webhook signature.
/// The `secret` parameter is the raw webhook secret stored in DB.
fn verify_signature(secret: &str, body: &[u8], signature: &str) -> bool {
  use hmac::{Hmac, Mac};
  use sha2::Sha256;

  let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
    return false;
  };
  mac.update(body);

  // Parse the hex signature (strip "sha256=" prefix if present)
  let hex_sig = signature
    .strip_prefix("sha256=")
    .or_else(|| signature.strip_prefix("sha1="))
    .unwrap_or(signature);

  let Ok(sig_bytes) = hex::decode(hex_sig) else {
    return false;
  };

  mac.verify_slice(&sig_bytes).is_ok()
}

async fn handle_github_push(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  // Check webhook config exists
  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    "github",
  )
  .await
  .map_err(ApiError)?;

  let webhook_config = match webhook_config {
    Some(c) => c,
    None => {
      return Ok((
        StatusCode::NOT_FOUND,
        Json(WebhookResponse {
          accepted: false,
          message:  "No GitHub webhook configured for this project".to_string(),
        }),
      ));
    },
  };

  // Verify signature if secret is configured
  if let Some(ref secret_hash) = webhook_config.secret_hash {
    let signature = headers
      .get("x-hub-signature-256")
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");

    if !verify_signature(secret_hash, &body, signature) {
      return Ok((
        StatusCode::UNAUTHORIZED,
        Json(WebhookResponse {
          accepted: false,
          message:  "Invalid webhook signature".to_string(),
        }),
      ));
    }
  }

  // Parse payload
  let payload: GithubPushPayload =
    serde_json::from_slice(&body).map_err(|e| {
      ApiError(fc_common::CiError::Validation(format!(
        "Invalid payload: {e}"
      )))
    })?;

  let commit = payload.after.unwrap_or_default();
  if commit.is_empty() || commit == "0000000000000000000000000000000000000000" {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Branch deletion event, skipping".to_string(),
      }),
    ));
  }

  // Find matching jobsets for this project and trigger evaluations
  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:   jobset.id,
      commit_hash: commit.clone(),
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(fc_common::CiError::Conflict(_)) => {}, // already exists
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for commit {commit}"
      ),
    }),
  ))
}

async fn handle_gitea_push(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  // Check webhook config exists
  let forge_type = if headers.get("x-forgejo-event").is_some() {
    "forgejo"
  } else {
    "gitea"
  };

  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    forge_type,
  )
  .await
  .map_err(ApiError)?;

  // Fall back to the other type if not found
  let webhook_config = match webhook_config {
    Some(c) => c,
    None => {
      let alt = if forge_type == "gitea" {
        "forgejo"
      } else {
        "gitea"
      };
      match repo::webhook_configs::get_by_project_and_forge(
        &state.pool,
        project_id,
        alt,
      )
      .await
      .map_err(ApiError)?
      {
        Some(c) => c,
        None => {
          return Ok((
            StatusCode::NOT_FOUND,
            Json(WebhookResponse {
              accepted: false,
              message:  "No Gitea/Forgejo webhook configured for this project"
                .to_string(),
            }),
          ));
        },
      }
    },
  };

  // Verify signature if configured
  if let Some(ref secret_hash) = webhook_config.secret_hash {
    let signature = headers
      .get("x-gitea-signature")
      .or_else(|| headers.get("x-forgejo-signature"))
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");

    if !verify_signature(secret_hash, &body, signature) {
      return Ok((
        StatusCode::UNAUTHORIZED,
        Json(WebhookResponse {
          accepted: false,
          message:  "Invalid webhook signature".to_string(),
        }),
      ));
    }
  }

  let payload: GiteaPushPayload =
    serde_json::from_slice(&body).map_err(|e| {
      ApiError(fc_common::CiError::Validation(format!(
        "Invalid payload: {e}"
      )))
    })?;

  let commit = payload.after.unwrap_or_default();
  if commit.is_empty() || commit == "0000000000000000000000000000000000000000" {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Branch deletion event, skipping".to_string(),
      }),
    ));
  }

  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:   jobset.id,
      commit_hash: commit.clone(),
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(fc_common::CiError::Conflict(_)) => {},
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for commit {commit}"
      ),
    }),
  ))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route(
      "/api/v1/webhooks/{project_id}/github",
      post(handle_github_push),
    )
    .route(
      "/api/v1/webhooks/{project_id}/gitea",
      post(handle_gitea_push),
    )
    .route(
      "/api/v1/webhooks/{project_id}/forgejo",
      post(handle_gitea_push),
    )
}

use axum::{
  extract::{FromRequestParts, Request, State},
  http::{StatusCode, request::Parts},
  middleware::Next,
  response::Response,
};
use fc_common::models::ApiKey;
use sha2::{Digest, Sha256};

use crate::state::AppState;

/// Extract and validate an API key from the Authorization header or session
/// cookie. Keys use the format: `Bearer fc_xxxx`. Session cookies use
/// `fc_session=<id>`. Write endpoints (POST/PUT/DELETE/PATCH) require a valid
/// key. Read endpoints (GET/HEAD/OPTIONS) try to extract optionally (for
/// dashboard admin UI).
pub async fn require_api_key(
  State(state): State<AppState>,
  mut request: Request,
  next: Next,
) -> Result<Response, StatusCode> {
  let method = request.method().clone();
  let is_read = method == axum::http::Method::GET
    || method == axum::http::Method::HEAD
    || method == axum::http::Method::OPTIONS;

  let auth_header = request
    .headers()
    .get("authorization")
    .and_then(|v| v.to_str().ok())
    .map(String::from);

  let token = auth_header
    .as_deref()
    .and_then(|h| h.strip_prefix("Bearer "));

  // Try Bearer token first
  if let Some(token) = token {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    if let Ok(Some(api_key)) =
      fc_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await
    {
      let pool = state.pool.clone();
      let key_id = api_key.id;
      tokio::spawn(async move {
        let _ = fc_common::repo::api_keys::touch_last_used(&pool, key_id).await;
      });

      request.extensions_mut().insert(api_key);
      return Ok(next.run(request).await);
    }
  }

  // Fall back to session cookie (so dashboard JS fetches work)
  if let Some(cookie_header) = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
    && let Some(session_id) = parse_cookie(cookie_header, "fc_session")
    && let Some(session) = state.sessions.get(&session_id)
    && session.created_at.elapsed()
      < std::time::Duration::from_secs(24 * 60 * 60)
  {
    request.extensions_mut().insert(session.api_key.clone());
    return Ok(next.run(request).await);
  }

  // No valid auth found
  if is_read {
    Ok(next.run(request).await)
  } else {
    Err(StatusCode::UNAUTHORIZED)
  }
}

/// Extractor that requires an authenticated admin user.
/// Use as a handler parameter: `_auth: RequireAdmin`
pub struct RequireAdmin(pub ApiKey);

impl FromRequestParts<AppState> for RequireAdmin {
  type Rejection = StatusCode;

  async fn from_request_parts(
    parts: &mut Parts,
    _state: &AppState,
  ) -> Result<Self, Self::Rejection> {
    let key = parts
      .extensions
      .get::<ApiKey>()
      .cloned()
      .ok_or(StatusCode::UNAUTHORIZED)?;
    if key.role == "admin" {
      Ok(RequireAdmin(key))
    } else {
      Err(StatusCode::FORBIDDEN)
    }
  }
}

/// Extractor that requires one of the specified roles (admin always passes).
/// Use as: `_auth: RequireRole<"cancel-build", "restart-jobs">`
///
/// Since const generics with strings aren't stable, use the helper function
/// instead.
pub struct RequireRoles(pub ApiKey);

impl RequireRoles {
  pub fn check(
    extensions: &axum::http::Extensions,
    allowed: &[&str],
  ) -> Result<ApiKey, StatusCode> {
    let key = extensions
      .get::<ApiKey>()
      .cloned()
      .ok_or(StatusCode::UNAUTHORIZED)?;
    if key.role == "admin" || allowed.contains(&key.role.as_str()) {
      Ok(key)
    } else {
      Err(StatusCode::FORBIDDEN)
    }
  }
}

/// Session extraction middleware for dashboard routes.
/// Reads `fc_session` cookie and inserts ApiKey into extensions if valid.
pub async fn extract_session(
  State(state): State<AppState>,
  mut request: Request,
  next: Next,
) -> Response {
  if let Some(cookie_header) = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
    && let Some(session_id) = parse_cookie(cookie_header, "fc_session")
    && let Some(session) = state.sessions.get(&session_id)
  {
    // Check session expiry (24 hours)
    if session.created_at.elapsed()
      < std::time::Duration::from_secs(24 * 60 * 60)
    {
      request.extensions_mut().insert(session.api_key.clone());
    } else {
      // Expired, remove it
      drop(session);
      state.sessions.remove(&session_id);
    }
  }
  next.run(request).await
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
  header
    .split(';')
    .filter_map(|pair| {
      let pair = pair.trim();
      let (k, v) = pair.split_once('=')?;
      if k.trim() == name {
        Some(v.trim().to_string())
      } else {
        None
      }
    })
    .next()
}

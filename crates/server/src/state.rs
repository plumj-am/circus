use std::{sync::Arc, time::Instant};

use dashmap::DashMap;
use fc_common::{
  config::Config,
  models::{ApiKey, User},
};
use sqlx::PgPool;

/// Maximum session lifetime before automatic eviction (24 hours).
const SESSION_MAX_AGE: std::time::Duration =
  std::time::Duration::from_hours(24);

/// How often the background cleanup task runs (every 5 minutes).
const SESSION_CLEANUP_INTERVAL: std::time::Duration =
  std::time::Duration::from_mins(5);

/// Session data supporting both API key and user authentication
#[derive(Clone)]
pub struct SessionData {
  pub api_key:    Option<ApiKey>,
  pub user:       Option<User>,
  pub created_at: Instant,
}

impl SessionData {
  /// Check if the session has admin role
  #[must_use]
  pub fn is_admin(&self) -> bool {
    self.user.as_ref().map_or_else(
      || self.api_key.as_ref().is_some_and(|key| key.role == "admin"),
      |user| user.role == "admin",
    )
  }

  /// Check if the session has a specific role
  #[must_use]
  pub fn has_role(&self, role: &str) -> bool {
    if self.is_admin() {
      return true;
    }
    self.user.as_ref().map_or_else(
      || self.api_key.as_ref().is_some_and(|key| key.role == role),
      |user| user.role == role,
    )
  }

  /// Get the display name for the session (username or api key name)
  #[must_use]
  pub fn display_name(&self) -> String {
    self.user.as_ref().map_or_else(
      || {
        self
          .api_key
          .as_ref()
          .map_or_else(|| "Anonymous".to_string(), |key| key.name.clone())
      },
      |user| user.username.clone(),
    )
  }

  /// Check if this is a user session (not just API key)
  #[must_use]
  pub const fn is_user_session(&self) -> bool {
    self.user.is_some()
  }
}

#[derive(Clone)]
pub struct AppState {
  pub pool:        PgPool,
  pub config:      Config,
  pub sessions:    Arc<DashMap<String, SessionData>>,
  pub http_client: reqwest::Client,
}

impl AppState {
  /// Spawn a background task that periodically evicts expired sessions.
  /// This prevents unbounded memory growth from the in-memory session store.
  pub fn spawn_session_cleanup(&self) {
    let sessions = self.sessions.clone();
    tokio::spawn(async move {
      loop {
        tokio::time::sleep(SESSION_CLEANUP_INTERVAL).await;
        let before = sessions.len();
        sessions
          .retain(|_, session| session.created_at.elapsed() < SESSION_MAX_AGE);
        let evicted = before.saturating_sub(sessions.len());
        if evicted > 0 {
          tracing::debug!(
            evicted = evicted,
            remaining = sessions.len(),
            "Evicted expired sessions"
          );
        }
      }
    });
  }
}

use std::{sync::Arc, time::Instant};

use dashmap::DashMap;
use fc_common::{
  config::Config,
  models::{ApiKey, User},
};
use sqlx::PgPool;

/// Session data supporting both API key and user authentication
#[derive(Clone)]
pub struct SessionData {
  pub api_key:    Option<ApiKey>,
  pub user:       Option<User>,
  pub created_at: Instant,
}

impl SessionData {
  /// Check if the session has admin role
  pub fn is_admin(&self) -> bool {
    if let Some(ref user) = self.user {
      user.role == "admin"
    } else if let Some(ref key) = self.api_key {
      key.role == "admin"
    } else {
      false
    }
  }

  /// Check if the session has a specific role
  pub fn has_role(&self, role: &str) -> bool {
    if self.is_admin() {
      return true;
    }
    if let Some(ref user) = self.user {
      user.role == role
    } else if let Some(ref key) = self.api_key {
      key.role == role
    } else {
      false
    }
  }

  /// Get the display name for the session (username or api key name)
  pub fn display_name(&self) -> String {
    if let Some(ref user) = self.user {
      user.username.clone()
    } else if let Some(ref key) = self.api_key {
      key.name.clone()
    } else {
      "Anonymous".to_string()
    }
  }

  /// Check if this is a user session (not just API key)
  pub fn is_user_session(&self) -> bool {
    self.user.is_some()
  }
}

#[derive(Clone)]
pub struct AppState {
  pub pool:     PgPool,
  pub config:   Config,
  pub sessions: Arc<DashMap<String, SessionData>>,
}

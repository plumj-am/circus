use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use fc_common::config::Config;
use fc_common::models::ApiKey;
use sqlx::PgPool;

pub struct SessionData {
    pub api_key: ApiKey,
    pub created_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Config,
    pub sessions: Arc<DashMap<String, SessionData>>,
}

//! Data models for CI

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub repository_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Jobset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub nix_expression: String,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub branch: Option<String>,
    pub scheduling_shares: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Evaluation {
    pub id: Uuid,
    pub jobset_id: Uuid,
    pub commit_hash: String,
    pub evaluation_time: DateTime<Utc>,
    pub status: EvaluationStatus,
    pub error_message: Option<String>,
    pub inputs_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum EvaluationStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Build {
    pub id: Uuid,
    pub evaluation_id: Uuid,
    pub job_name: String,
    pub drv_path: String,
    pub status: BuildStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub log_path: Option<String>,
    pub build_output_path: Option<String>,
    pub error_message: Option<String>,
    pub system: Option<String>,
    pub priority: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub notification_pending_since: Option<DateTime<Utc>>,
    pub log_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub outputs: Option<serde_json::Value>,
    pub is_aggregate: bool,
    pub constituents: Option<serde_json::Value>,
    pub builder_id: Option<Uuid>,
    pub signed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, PartialEq)]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum BuildStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildProduct {
    pub id: Uuid,
    pub build_id: Uuid,
    pub name: String,
    pub path: String,
    pub sha256_hash: Option<String>,
    pub file_size: Option<i64>,
    pub content_type: Option<String>,
    pub is_directory: bool,
    pub gc_root_path: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildStep {
    pub id: Uuid,
    pub build_id: Uuid,
    pub step_number: i32,
    pub command: String,
    pub output: Option<String>,
    pub error_output: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuildDependency {
    pub id: Uuid,
    pub build_id: Uuid,
    pub dependency_build_id: Uuid,
}

/// Active jobset view — enabled jobsets joined with project info.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ActiveJobset {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub nix_expression: String,
    pub enabled: bool,
    pub flake_mode: bool,
    pub check_interval: i32,
    pub branch: Option<String>,
    pub scheduling_shares: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub project_name: String,
    pub repository_url: String,
}

/// Build statistics from the build_stats view.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, Default)]
pub struct BuildStats {
    pub total_builds: Option<i64>,
    pub completed_builds: Option<i64>,
    pub failed_builds: Option<i64>,
    pub running_builds: Option<i64>,
    pub pending_builds: Option<i64>,
    pub avg_duration_seconds: Option<f64>,
}

/// API key for authentication.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Webhook configuration for a project.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct WebhookConfig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub forge_type: String,
    pub secret_hash: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Notification configuration for a project.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NotificationConfig {
    pub id: Uuid,
    pub project_id: Uuid,
    pub notification_type: String,
    pub config: serde_json::Value,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// Jobset input definition.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct JobsetInput {
    pub id: Uuid,
    pub jobset_id: Uuid,
    pub name: String,
    pub input_type: String,
    pub value: String,
    pub revision: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Release channel — tracks the latest "good" evaluation for a jobset.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Channel {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub jobset_id: Uuid,
    pub current_evaluation_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Remote builder for multi-machine / multi-arch builds.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct RemoteBuilder {
    pub id: Uuid,
    pub name: String,
    pub ssh_uri: String,
    pub systems: Vec<String>,
    pub max_jobs: i32,
    pub speed_factor: i32,
    pub supported_features: Vec<String>,
    pub mandatory_features: Vec<String>,
    pub enabled: bool,
    pub public_host_key: Option<String>,
    pub ssh_key_file: Option<String>,
    pub created_at: DateTime<Utc>,
}

// --- Pagination ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl PaginationParams {
    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(50).min(200).max(1)
    }

    pub fn offset(&self) -> i64 {
        self.offset.unwrap_or(0).max(0)
    }
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            limit: Some(50),
            offset: Some(0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

// --- DTO structs for creation and updates ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProject {
    pub name: String,
    pub description: Option<String>,
    pub repository_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateProject {
    pub name: Option<String>,
    pub description: Option<String>,
    pub repository_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateJobset {
    pub project_id: Uuid,
    pub name: String,
    pub nix_expression: String,
    pub enabled: Option<bool>,
    pub flake_mode: Option<bool>,
    pub check_interval: Option<i32>,
    pub branch: Option<String>,
    pub scheduling_shares: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateJobset {
    pub name: Option<String>,
    pub nix_expression: Option<String>,
    pub enabled: Option<bool>,
    pub flake_mode: Option<bool>,
    pub check_interval: Option<i32>,
    pub branch: Option<String>,
    pub scheduling_shares: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEvaluation {
    pub jobset_id: Uuid,
    pub commit_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuild {
    pub evaluation_id: Uuid,
    pub job_name: String,
    pub drv_path: String,
    pub system: Option<String>,
    pub outputs: Option<serde_json::Value>,
    pub is_aggregate: Option<bool>,
    pub constituents: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuildProduct {
    pub build_id: Uuid,
    pub name: String,
    pub path: String,
    pub sha256_hash: Option<String>,
    pub file_size: Option<i64>,
    pub content_type: Option<String>,
    pub is_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateBuildStep {
    pub build_id: Uuid,
    pub step_number: i32,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateWebhookConfig {
    pub project_id: Uuid,
    pub forge_type: String,
    pub secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationConfig {
    pub project_id: Uuid,
    pub notification_type: String,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChannel {
    pub project_id: Uuid,
    pub name: String,
    pub jobset_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateChannel {
    pub name: Option<String>,
    pub jobset_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteBuilder {
    pub name: String,
    pub ssh_uri: String,
    pub systems: Vec<String>,
    pub max_jobs: Option<i32>,
    pub speed_factor: Option<i32>,
    pub supported_features: Option<Vec<String>>,
    pub mandatory_features: Option<Vec<String>>,
    pub public_host_key: Option<String>,
    pub ssh_key_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRemoteBuilder {
    pub name: Option<String>,
    pub ssh_uri: Option<String>,
    pub systems: Option<Vec<String>>,
    pub max_jobs: Option<i32>,
    pub speed_factor: Option<i32>,
    pub supported_features: Option<Vec<String>>,
    pub mandatory_features: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub public_host_key: Option<String>,
    pub ssh_key_file: Option<String>,
}

/// Summary of system status for the admin API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStatus {
    pub projects_count: i64,
    pub jobsets_count: i64,
    pub evaluations_count: i64,
    pub builds_pending: i64,
    pub builds_running: i64,
    pub builds_completed: i64,
    pub builds_failed: i64,
    pub remote_builders: i64,
    pub channels_count: i64,
}

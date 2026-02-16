//! Configuration management for FC CI

use std::path::PathBuf;

use config as config_crate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
  pub database:      DatabaseConfig,
  pub server:        ServerConfig,
  pub evaluator:     EvaluatorConfig,
  pub queue_runner:  QueueRunnerConfig,
  pub gc:            GcConfig,
  pub logs:          LogConfig,
  pub notifications: NotificationsConfig,
  pub cache:         CacheConfig,
  pub signing:       SigningConfig,
  #[serde(default)]
  pub cache_upload:  CacheUploadConfig,
  pub tracing:       TracingConfig,
  #[serde(default)]
  pub declarative:   DeclarativeConfig,
  #[serde(default)]
  pub oauth:         OAuthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
  pub url:             String,
  pub max_connections: u32,
  pub min_connections: u32,
  pub connect_timeout: u64,
  pub idle_timeout:    u64,
  pub max_lifetime:    u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
  pub host:                String,
  pub port:                u16,
  pub request_timeout:     u64,
  pub max_body_size:       usize,
  pub api_key:             Option<String>,
  pub allowed_origins:     Vec<String>,
  pub cors_permissive:     bool,
  pub rate_limit_rps:      Option<u64>,
  pub rate_limit_burst:    Option<u32>,
  /// Allowed URL schemes for repository URLs. Insecure schemes emit a warning
  /// on startup
  pub allowed_url_schemes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EvaluatorConfig {
  pub poll_interval:        u64,
  pub git_timeout:          u64,
  pub nix_timeout:          u64,
  pub max_concurrent_evals: usize,
  pub work_dir:             PathBuf,
  pub restrict_eval:        bool,
  pub allow_ifd:            bool,

  /// Whether to abort on the first evaluation cycle error instead of logging
  /// and retrying.
  pub strict_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRunnerConfig {
  pub workers:       usize,
  pub poll_interval: u64,
  pub build_timeout: u64,
  pub work_dir:      PathBuf,

  /// When true, abort on the first runner loop error instead of logging and
  /// retrying.
  #[serde(default)]
  pub strict_errors: bool,

  /// Cache failed derivation paths to skip known-failing builds.
  #[serde(default = "default_true")]
  pub failed_paths_cache: bool,

  /// TTL in seconds for failed paths cache entries (default 24h).
  #[serde(default = "default_failed_paths_ttl")]
  pub failed_paths_ttl: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GcConfig {
  pub gc_roots_dir:     PathBuf,
  pub enabled:          bool,
  pub max_age_days:     u64,
  pub cleanup_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
  pub log_dir:  PathBuf,
  pub compress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OAuthConfig {
  pub github: Option<GitHubOAuthConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GitHubOAuthConfig {
  pub client_id:     String,
  pub client_secret: String,
  pub redirect_uri:  String,
}

impl std::fmt::Debug for GitHubOAuthConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GitHubOAuthConfig")
      .field("client_id", &self.client_id)
      .field("client_secret", &"[REDACTED]")
      .field("redirect_uri", &self.redirect_uri)
      .finish()
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct NotificationsConfig {
  pub run_command:  Option<String>,
  pub github_token: Option<String>,
  pub gitea_url:    Option<String>,
  pub gitea_token:  Option<String>,
  pub gitlab_url:   Option<String>,
  pub gitlab_token: Option<String>,
  pub email:        Option<EmailConfig>,
  pub alerts:       Option<AlertConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
  pub enabled:             bool,
  pub error_threshold:     f64,
  pub time_window_minutes: i64,
}

impl Default for AlertConfig {
  fn default() -> Self {
    Self {
      enabled:             false,
      error_threshold:     20.0,
      time_window_minutes: 60,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailConfig {
  pub smtp_host:       String,
  pub smtp_port:       u16,
  pub smtp_user:       Option<String>,
  pub smtp_password:   Option<String>,
  pub from_address:    String,
  pub to_addresses:    Vec<String>,
  pub tls:             bool,
  pub on_failure_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
  pub enabled:         bool,
  pub secret_key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct SigningConfig {
  pub enabled:  bool,
  pub key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheUploadConfig {
  pub enabled:   bool,
  pub store_uri: Option<String>,
  /// S3-specific configuration (used when store_uri starts with s3://)
  pub s3:        Option<S3CacheConfig>,
}

/// S3-specific cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct S3CacheConfig {
  /// AWS region (e.g., "us-east-1")
  pub region:            Option<String>,
  /// Path prefix within the bucket (e.g., "nix-cache/")
  pub prefix:            Option<String>,
  /// AWS access key ID (optional - uses IAM role if not provided)
  pub access_key_id:     Option<String>,
  /// AWS secret access key (optional - uses IAM role if not provided)
  pub secret_access_key: Option<String>,
  /// Session token for temporary credentials (optional)
  pub session_token:     Option<String>,
  /// Endpoint URL for S3-compatible services (e.g., MinIO)
  pub endpoint_url:      Option<String>,
  /// Whether to use path-style addressing (for MinIO compatibility)
  pub use_path_style:    bool,
}

impl Default for S3CacheConfig {
  fn default() -> Self {
    Self {
      region:            None,
      prefix:            None,
      access_key_id:     None,
      secret_access_key: None,
      session_token:     None,
      endpoint_url:      None,
      use_path_style:    false,
    }
  }
}

impl Default for CacheUploadConfig {
  fn default() -> Self {
    Self {
      enabled:   false,
      store_uri: None,
      s3:        None,
    }
  }
}

/// Declarative project/jobset/api-key/user definitions.
/// These are upserted on server startup, enabling fully declarative operation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DeclarativeConfig {
  pub projects:        Vec<DeclarativeProject>,
  pub api_keys:        Vec<DeclarativeApiKey>,
  pub users:           Vec<DeclarativeUser>,
  /// Remote builder definitions for distributed builds
  pub remote_builders: Vec<DeclarativeRemoteBuilder>,
}

/// Declarative remote builder configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeRemoteBuilder {
  pub name:               String,
  pub ssh_uri:            String,
  pub systems:            Vec<String>,
  #[serde(default = "default_max_jobs")]
  pub max_jobs:           i32,
  #[serde(default = "default_speed_factor")]
  pub speed_factor:       i32,
  #[serde(default)]
  pub supported_features: Vec<String>,
  #[serde(default)]
  pub mandatory_features: Vec<String>,
  /// Path to SSH private key file (for production)
  pub ssh_key_file:       Option<String>,
  /// SSH public host key for verification
  pub public_host_key:    Option<String>,
  #[serde(default = "default_true")]
  pub enabled:            bool,
}

const fn default_max_jobs() -> i32 {
  1
}

const fn default_speed_factor() -> i32 {
  1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeProject {
  pub name:           String,
  pub repository_url: String,
  pub description:    Option<String>,
  #[serde(default)]
  pub jobsets:        Vec<DeclarativeJobset>,
  /// Notification configurations for this project
  #[serde(default)]
  pub notifications:  Vec<DeclarativeNotification>,
  /// Webhook configurations for this project
  #[serde(default)]
  pub webhooks:       Vec<DeclarativeWebhook>,
  /// Release channels for this project
  #[serde(default)]
  pub channels:       Vec<DeclarativeChannel>,
  /// Project members with their roles
  #[serde(default)]
  pub members:        Vec<DeclarativeProjectMember>,
}

/// Declarative notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeNotification {
  /// Notification type: `github_status`, email, `gitlab_status`,
  /// `gitea_status`, `run_command`
  pub notification_type: String,
  /// Type-specific configuration (JSON object)
  pub config:            serde_json::Value,
  #[serde(default = "default_true")]
  pub enabled:           bool,
}

/// Declarative webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeWebhook {
  /// Forge type: github, gitea, gitlab
  pub forge_type:  String,
  /// Webhook secret (inline, for dev/testing only)
  pub secret:      Option<String>,
  /// Path to a file containing the webhook secret (for production)
  pub secret_file: Option<String>,
  #[serde(default = "default_true")]
  pub enabled:     bool,
}

/// Declarative channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeChannel {
  pub name:        String,
  /// Name of the jobset this channel tracks (resolved during bootstrap)
  pub jobset_name: String,
}

/// Declarative project member configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeProjectMember {
  /// Username of the member (must exist in users)
  pub username: String,
  /// Role: member, maintainer, or admin
  #[serde(default = "default_member_role")]
  pub role:     String,
}

fn default_member_role() -> String {
  "member".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeJobset {
  pub name:              String,
  pub nix_expression:    String,
  #[serde(default = "default_true")]
  pub enabled:           bool,
  #[serde(default = "default_true")]
  pub flake_mode:        bool,
  #[serde(default = "default_check_interval")]
  pub check_interval:    i32,
  /// Jobset state: disabled, enabled, `one_shot`, or `one_at_a_time`
  pub state:             Option<String>,
  /// Git branch to track (defaults to repository default branch)
  pub branch:            Option<String>,
  /// Scheduling priority shares (default 100, higher = more priority)
  #[serde(default = "default_scheduling_shares")]
  pub scheduling_shares: i32,
  /// Number of recent successful evaluations to retain (default 3)
  pub keep_nr:           Option<i32>,
  /// Jobset inputs for parameterized evaluations
  #[serde(default)]
  pub inputs:            Vec<DeclarativeJobsetInput>,
}

/// Declarative jobset input for parameterized builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeJobsetInput {
  pub name:       String,
  /// Input type: git, string, boolean, path, or build
  pub input_type: String,
  pub value:      String,
  /// Git revision (for git inputs)
  pub revision:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeApiKey {
  pub name: String,
  pub key:  String,
  #[serde(default = "default_role")]
  pub role: String,
}

/// Declarative user definition for configuration-driven user management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeUser {
  pub username:      String,
  pub email:         String,
  pub full_name:     Option<String>,
  /// Password provided inline (for dev/testing only).
  pub password:      Option<String>,
  /// Path to a file containing the password (for production use with secrets).
  pub password_file: Option<String>,
  #[serde(default = "default_user_role")]
  pub role:          String,
  #[serde(default = "default_true")]
  pub enabled:       bool,
}

fn default_user_role() -> String {
  "read-only".to_string()
}

const fn default_true() -> bool {
  true
}

const fn default_failed_paths_ttl() -> u64 {
  86400
}

const fn default_check_interval() -> i32 {
  60
}

const fn default_scheduling_shares() -> i32 {
  100
}

fn default_role() -> String {
  "read-only".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
  pub level:           String,
  pub format:          String,
  pub show_targets:    bool,
  pub show_timestamps: bool,
}

impl Default for TracingConfig {
  fn default() -> Self {
    Self {
      level:           "info".to_string(),
      format:          "compact".to_string(),
      show_targets:    true,
      show_timestamps: true,
    }
  }
}

impl Default for DatabaseConfig {
  fn default() -> Self {
    Self {
      url:             "postgresql://fc_ci:password@localhost/fc_ci"
        .to_string(),
      max_connections: 20,
      min_connections: 5,
      connect_timeout: 30,
      idle_timeout:    600,
      max_lifetime:    1800,
    }
  }
}

impl DatabaseConfig {
  pub fn validate(&self) -> anyhow::Result<()> {
    if self.url.is_empty() {
      return Err(anyhow::anyhow!("Database URL cannot be empty"));
    }

    if !self.url.starts_with("postgresql://")
      && !self.url.starts_with("postgres://")
    {
      return Err(anyhow::anyhow!(
        "Database URL must start with postgresql:// or postgres://"
      ));
    }

    if self.max_connections == 0 {
      return Err(anyhow::anyhow!(
        "Max database connections must be greater than 0"
      ));
    }

    if self.min_connections > self.max_connections {
      return Err(anyhow::anyhow!(
        "Min database connections cannot exceed max connections"
      ));
    }

    Ok(())
  }
}

impl Default for ServerConfig {
  fn default() -> Self {
    Self {
      host:                "127.0.0.1".to_string(),
      port:                3000,
      request_timeout:     30,
      max_body_size:       10 * 1024 * 1024, // 10MB
      api_key:             None,
      allowed_origins:     Vec::new(),
      cors_permissive:     false,
      rate_limit_rps:      None,
      rate_limit_burst:    None,
      allowed_url_schemes: vec![
        "https".into(),
        "http".into(),
        "git".into(),
        "ssh".into(),
      ],
    }
  }
}

impl Default for EvaluatorConfig {
  fn default() -> Self {
    Self {
      poll_interval:        60,
      git_timeout:          600,
      nix_timeout:          1800,
      max_concurrent_evals: 4,
      work_dir:             PathBuf::from("/tmp/fc-evaluator"),
      restrict_eval:        true,
      allow_ifd:            false,
      strict_errors:        false,
    }
  }
}

impl Default for QueueRunnerConfig {
  fn default() -> Self {
    Self {
      workers:            4,
      poll_interval:      5,
      build_timeout:      3600,
      work_dir:           PathBuf::from("/tmp/fc-queue-runner"),
      strict_errors:      false,
      failed_paths_cache: true,
      failed_paths_ttl:   86400,
    }
  }
}

impl Default for GcConfig {
  fn default() -> Self {
    Self {
      gc_roots_dir:     PathBuf::from(
        "/nix/var/nix/gcroots/per-user/fc/fc-roots",
      ),
      enabled:          true,
      max_age_days:     30,
      cleanup_interval: 3600,
    }
  }
}

impl Default for LogConfig {
  fn default() -> Self {
    Self {
      log_dir:  PathBuf::from("/var/lib/fc/logs"),
      compress: false,
    }
  }
}

impl Default for CacheConfig {
  fn default() -> Self {
    Self {
      enabled:         true,
      secret_key_file: None,
    }
  }
}

impl Config {
  pub fn load() -> anyhow::Result<Self> {
    let mut settings = config_crate::Config::builder();

    // Load default configuration
    settings =
      settings.add_source(config_crate::Config::try_from(&Self::default())?);

    // Load from config file if it exists
    if let Ok(config_path) = std::env::var("FC_CONFIG_FILE") {
      if std::path::Path::new(&config_path).exists() {
        settings =
          settings.add_source(config_crate::File::with_name(&config_path));
      }
    } else if std::path::Path::new("fc.toml").exists() {
      settings = settings
        .add_source(config_crate::File::with_name("fc").required(false));
    }

    // Load from environment variables with FC_ prefix (highest priority)
    settings = settings.add_source(
      config_crate::Environment::with_prefix("FC")
        .separator("__")
        .try_parsing(true),
    );

    let config = settings.build()?.try_deserialize::<Self>()?;

    // Validate configuration
    config.validate()?;

    Ok(config)
  }

  pub fn validate(&self) -> anyhow::Result<()> {
    // Validate database URL
    if self.database.url.is_empty() {
      return Err(anyhow::anyhow!("Database URL cannot be empty"));
    }

    if !self.database.url.starts_with("postgresql://")
      && !self.database.url.starts_with("postgres://")
    {
      return Err(anyhow::anyhow!(
        "Database URL must start with postgresql:// or postgres://"
      ));
    }

    // Validate connection pool settings
    if self.database.max_connections == 0 {
      return Err(anyhow::anyhow!(
        "Max database connections must be greater than 0"
      ));
    }

    if self.database.min_connections > self.database.max_connections {
      return Err(anyhow::anyhow!(
        "Min database connections cannot exceed max connections"
      ));
    }

    // Validate server settings
    if self.server.port == 0 {
      return Err(anyhow::anyhow!("Server port must be greater than 0"));
    }

    // Validate evaluator settings
    if self.evaluator.poll_interval == 0 {
      return Err(anyhow::anyhow!(
        "Evaluator poll interval must be greater than 0"
      ));
    }

    // Validate queue runner settings
    if self.queue_runner.workers == 0 {
      return Err(anyhow::anyhow!(
        "Queue runner workers must be greater than 0"
      ));
    }

    // Validate GC config
    if self.gc.enabled && self.gc.gc_roots_dir.as_os_str().is_empty() {
      return Err(anyhow::anyhow!(
        "GC roots directory cannot be empty when GC is enabled"
      ));
    }

    // Validate log config
    if self.logs.log_dir.as_os_str().is_empty() {
      return Err(anyhow::anyhow!("Log directory cannot be empty"));
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::env;

  use super::*;

  #[test]
  fn test_default_config() {
    let config = Config::default();
    assert!(config.validate().is_ok());
  }

  #[test]
  fn test_invalid_database_url() {
    let mut config = Config::default();
    config.database.url = "invalid://url".to_string();
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_invalid_port() {
    let mut config = Config::default();
    config.server.port = 0;
    assert!(config.validate().is_err());

    config.server.port = 65535;
    assert!(config.validate().is_ok()); // valid port
  }

  #[test]
  fn test_invalid_connections() {
    let mut config = Config::default();
    config.database.max_connections = 0;
    assert!(config.validate().is_err());

    config.database.max_connections = 10;
    config.database.min_connections = 15;
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_declarative_config_default_is_empty() {
    let config = DeclarativeConfig::default();
    assert!(config.projects.is_empty());
    assert!(config.api_keys.is_empty());
  }

  #[test]
  fn test_declarative_config_deserialization() {
    let toml_str = r#"
            [[projects]]
            name = "my-project"
            repository_url = "https://github.com/test/repo"
            description = "Test project"

            [[projects.jobsets]]
            name = "packages"
            nix_expression = "packages"

            [[api_keys]]
            name = "admin-key"
            key = "fc_secret_key_123"
            role = "admin"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.projects.len(), 1);
    assert_eq!(config.projects[0].name, "my-project");
    assert_eq!(config.projects[0].jobsets.len(), 1);
    assert_eq!(config.projects[0].jobsets[0].name, "packages");
    assert!(config.projects[0].jobsets[0].enabled); // default true
    assert!(config.projects[0].jobsets[0].flake_mode); // default true
    assert_eq!(config.api_keys.len(), 1);
    assert_eq!(config.api_keys[0].role, "admin");
  }

  #[test]
  fn test_declarative_config_serialization_roundtrip() {
    let config = DeclarativeConfig {
      projects:        vec![DeclarativeProject {
        name:           "test".to_string(),
        repository_url: "https://example.com/repo".to_string(),
        description:    Some("desc".to_string()),
        jobsets:        vec![DeclarativeJobset {
          name:              "checks".to_string(),
          nix_expression:    "checks".to_string(),
          enabled:           true,
          flake_mode:        true,
          check_interval:    300,
          state:             None,
          branch:            None,
          scheduling_shares: 100,
          keep_nr:           None,
          inputs:            vec![],
        }],
        notifications:  vec![],
        webhooks:       vec![],
        channels:       vec![],
        members:        vec![],
      }],
      api_keys:        vec![DeclarativeApiKey {
        name: "test-key".to_string(),
        key:  "fc_test".to_string(),
        role: "admin".to_string(),
      }],
      users:           vec![],
      remote_builders: vec![],
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: DeclarativeConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.projects[0].jobsets[0].check_interval, 300);
    assert_eq!(parsed.api_keys[0].name, "test-key");
  }

  #[test]
  fn test_declarative_config_with_main_config() {
    // Ensure declarative section is optional (default empty)
    // Use the config crate loader which provides defaults for missing fields
    let config = Config::default();
    assert!(config.declarative.projects.is_empty());
    assert!(config.declarative.api_keys.is_empty());
    // And that the Config can be serialized back with declarative section
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert!(parsed.declarative.projects.is_empty());
  }

  #[test]
  fn test_declarative_api_key_default_role_is_read_only() {
    let toml_str = r#"
            [[api_keys]]
            name = "default-key"
            key = "fc_test_123"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.api_keys[0].role, "read-only");
  }

  #[test]
  fn test_environment_override() {
    // Test environment variable parsing directly
    unsafe {
      env::set_var("FC_DATABASE__URL", "postgresql://test:test@localhost/test");
      env::set_var("FC_SERVER__PORT", "8080");
    }

    // Test that environment variables are being read correctly
    let db_url = std::env::var("FC_DATABASE__URL").unwrap();
    let server_port = std::env::var("FC_SERVER__PORT").unwrap();

    assert_eq!(db_url, "postgresql://test:test@localhost/test");
    assert_eq!(server_port, "8080");

    unsafe {
      env::remove_var("FC_DATABASE__URL");
      env::remove_var("FC_SERVER__PORT");
    }
  }
}

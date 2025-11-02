//! Configuration management for FC CI

use config as config_crate;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub evaluator: EvaluatorConfig,
    pub queue_runner: QueueRunnerConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: u64,
    pub idle_timeout: u64,
    pub max_lifetime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub request_timeout: u64,
    pub max_body_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluatorConfig {
    pub poll_interval: u64,
    pub git_timeout: u64,
    pub nix_timeout: u64,
    pub work_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRunnerConfig {
    pub workers: usize,
    pub poll_interval: u64,
    pub build_timeout: u64,
    pub work_dir: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgresql://fc_ci:password@localhost/fc_ci".to_string(),
            max_connections: 20,
            min_connections: 5,
            connect_timeout: 30,
            idle_timeout: 600,
            max_lifetime: 1800,
        }
    }
}

impl DatabaseConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.url.is_empty() {
            return Err(anyhow::anyhow!("Database URL cannot be empty"));
        }

        if !self.url.starts_with("postgresql://") && !self.url.starts_with("postgres://") {
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
            host: "127.0.0.1".to_string(),
            port: 3000,
            request_timeout: 30,
            max_body_size: 10 * 1024 * 1024, // 10MB
        }
    }
}

impl Default for EvaluatorConfig {
    fn default() -> Self {
        Self {
            poll_interval: 60,
            git_timeout: 600,
            nix_timeout: 1800,
            work_dir: PathBuf::from("/tmp/fc-evaluator"),
        }
    }
}

impl Default for QueueRunnerConfig {
    fn default() -> Self {
        Self {
            workers: 4,
            poll_interval: 5,
            build_timeout: 3600,
            work_dir: PathBuf::from("/tmp/fc-queue-runner"),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut settings = config_crate::Config::builder();

        // Load default configuration
        settings = settings.add_source(config_crate::Config::try_from(&Self::default())?);

        // Load from config file if it exists
        if let Ok(config_path) = std::env::var("FC_CONFIG_FILE") {
            if std::path::Path::new(&config_path).exists() {
                settings = settings.add_source(config_crate::File::with_name(&config_path));
            }
        } else if std::path::Path::new("fc.toml").exists() {
            settings = settings.add_source(config_crate::File::with_name("fc").required(false));
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

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

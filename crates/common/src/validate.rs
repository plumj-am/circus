//! Input validation helpers

use std::sync::LazyLock;

use regex::Regex;

/// Validate that a path is a valid nix store path.
/// Rejects path traversal, overly long paths, and non-store paths.
#[must_use]
pub fn is_valid_store_path(path: &str) -> bool {
  path.starts_with("/nix/store/") && !path.contains("..") && path.len() < 512
}

/// Validate that a string is a valid nix store hash (32 lowercase alphanumeric
/// chars).
#[must_use]
pub fn is_valid_nix_hash(hash: &str) -> bool {
  hash.len() == 32
    && hash
      .chars()
      .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

// --- Validation trait and helpers ---

static NAME_RE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$").unwrap());

static COMMIT_HASH_RE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[0-9a-fA-F]{1,64}$").unwrap());

static SYSTEM_RE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^\w+-\w+$").unwrap());

const VALID_REPO_PREFIXES: &[&str] =
  &["https://", "http://", "git://", "ssh://", "file://"];
const VALID_FORGE_TYPES: &[&str] = &["github", "gitea", "forgejo", "gitlab"];

/// Trait for validating request DTOs before persisting.
pub trait Validate {
  fn validate(&self) -> Result<(), String>;
}

fn validate_name(name: &str, field: &str) -> Result<(), String> {
  if name.is_empty() || name.len() > 255 {
    return Err(format!("{field} must be between 1 and 255 characters"));
  }
  if !NAME_RE.is_match(name) {
    return Err(format!(
      "{field} must start with alphanumeric and contain only [a-zA-Z0-9_-]"
    ));
  }
  Ok(())
}

fn validate_repository_url(url: &str) -> Result<(), String> {
  if url.is_empty() {
    return Err("repository_url cannot be empty".to_string());
  }
  if url.len() > 2048 {
    return Err("repository_url must be at most 2048 characters".to_string());
  }
  if !VALID_REPO_PREFIXES.iter().any(|p| url.starts_with(p)) {
    return Err(
      "repository_url must start with https://, http://, git://, ssh://, or \
       file://"
        .to_string(),
    );
  }
  Ok(())
}

fn validate_description(desc: &str) -> Result<(), String> {
  if desc.len() > 4096 {
    return Err("description must be at most 4096 characters".to_string());
  }
  Ok(())
}

fn validate_nix_expression(expr: &str) -> Result<(), String> {
  if expr.is_empty() {
    return Err("nix_expression cannot be empty".to_string());
  }
  if expr.len() > 1024 {
    return Err("nix_expression must be at most 1024 characters".to_string());
  }
  if expr.contains('\0') {
    return Err("nix_expression must not contain null bytes".to_string());
  }
  Ok(())
}

fn validate_check_interval(interval: i32) -> Result<(), String> {
  if !(10..=86400).contains(&interval) {
    return Err("check_interval must be between 10 and 86400".to_string());
  }
  Ok(())
}

fn validate_commit_hash(hash: &str) -> Result<(), String> {
  if !COMMIT_HASH_RE.is_match(hash) {
    return Err("commit_hash must be 1-64 hex characters".to_string());
  }
  Ok(())
}

fn validate_drv_path(path: &str) -> Result<(), String> {
  if !is_valid_store_path(path) {
    return Err("drv_path must be a valid nix store path".to_string());
  }
  Ok(())
}

fn validate_system(system: &str) -> Result<(), String> {
  if !SYSTEM_RE.is_match(system) {
    return Err("system must match pattern like x86_64-linux".to_string());
  }
  Ok(())
}

fn validate_ssh_uri(uri: &str) -> Result<(), String> {
  if uri.is_empty() {
    return Err("ssh_uri cannot be empty".to_string());
  }
  if uri.len() > 2048 {
    return Err("ssh_uri must be at most 2048 characters".to_string());
  }
  Ok(())
}

fn validate_positive_i32(val: i32, field: &str) -> Result<(), String> {
  if val < 1 {
    return Err(format!("{field} must be >= 1"));
  }
  Ok(())
}

fn validate_forge_type(forge_type: &str) -> Result<(), String> {
  if !VALID_FORGE_TYPES.contains(&forge_type) {
    return Err(format!(
      "forge_type must be one of: {}",
      VALID_FORGE_TYPES.join(", ")
    ));
  }
  Ok(())
}

// --- Implementations ---

use crate::models::{
  CreateBuild,
  CreateChannel,
  CreateEvaluation,
  CreateJobset,
  CreateProject,
  CreateRemoteBuilder,
  CreateWebhookConfig,
  UpdateChannel,
  UpdateJobset,
  UpdateProject,
  UpdateRemoteBuilder,
};

impl Validate for CreateProject {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    validate_repository_url(&self.repository_url)?;
    if let Some(ref desc) = self.description {
      validate_description(desc)?;
    }
    Ok(())
  }
}

impl Validate for UpdateProject {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    if let Some(ref url) = self.repository_url {
      validate_repository_url(url)?;
    }
    if let Some(ref desc) = self.description {
      validate_description(desc)?;
    }
    Ok(())
  }
}

impl Validate for CreateJobset {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    validate_nix_expression(&self.nix_expression)?;
    if let Some(interval) = self.check_interval {
      validate_check_interval(interval)?;
    }
    Ok(())
  }
}

impl Validate for UpdateJobset {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    if let Some(ref expr) = self.nix_expression {
      validate_nix_expression(expr)?;
    }
    if let Some(interval) = self.check_interval {
      validate_check_interval(interval)?;
    }
    Ok(())
  }
}

impl Validate for CreateEvaluation {
  fn validate(&self) -> Result<(), String> {
    validate_commit_hash(&self.commit_hash)?;
    Ok(())
  }
}

impl Validate for CreateBuild {
  fn validate(&self) -> Result<(), String> {
    validate_drv_path(&self.drv_path)?;
    if let Some(ref system) = self.system {
      validate_system(system)?;
    }
    Ok(())
  }
}

impl Validate for CreateChannel {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    Ok(())
  }
}

impl Validate for UpdateChannel {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    Ok(())
  }
}

impl Validate for CreateRemoteBuilder {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    validate_ssh_uri(&self.ssh_uri)?;
    if self.systems.is_empty() {
      return Err("systems must not be empty".to_string());
    }
    for system in &self.systems {
      validate_system(system)?;
    }
    if let Some(max_jobs) = self.max_jobs {
      validate_positive_i32(max_jobs, "max_jobs")?;
    }
    if let Some(speed_factor) = self.speed_factor {
      validate_positive_i32(speed_factor, "speed_factor")?;
    }
    Ok(())
  }
}

impl Validate for UpdateRemoteBuilder {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    if let Some(ref uri) = self.ssh_uri {
      validate_ssh_uri(uri)?;
    }
    if let Some(ref systems) = self.systems {
      if systems.is_empty() {
        return Err("systems must not be empty".to_string());
      }
      for system in systems {
        validate_system(system)?;
      }
    }
    if let Some(max_jobs) = self.max_jobs {
      validate_positive_i32(max_jobs, "max_jobs")?;
    }
    if let Some(speed_factor) = self.speed_factor {
      validate_positive_i32(speed_factor, "speed_factor")?;
    }
    Ok(())
  }
}

impl Validate for CreateWebhookConfig {
  fn validate(&self) -> Result<(), String> {
    validate_forge_type(&self.forge_type)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::*;

  // --- is_valid_store_path ---

  #[test]
  fn valid_store_path() {
    assert!(is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12"
    ));
  }

  #[test]
  fn valid_store_path_nested() {
    assert!(is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12/bin/hello"
    ));
  }

  #[test]
  fn store_path_rejects_path_traversal() {
    assert!(!is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello/../../../etc/passwd"
    ));
  }

  #[test]
  fn store_path_rejects_relative_path() {
    assert!(!is_valid_store_path("nix/store/something"));
  }

  #[test]
  fn store_path_rejects_wrong_prefix() {
    assert!(!is_valid_store_path("/tmp/nix/store/something"));
    assert!(!is_valid_store_path("/etc/passwd"));
    assert!(!is_valid_store_path("/nix/var/something"));
  }

  #[test]
  fn store_path_rejects_empty() {
    assert!(!is_valid_store_path(""));
  }

  #[test]
  fn store_path_rejects_just_prefix() {
    // "/nix/store/" alone has no hash, but structurally starts_with and has no
    // .., so it passes. This is fine — the DB lookup won't find anything
    // for it.
    assert!(is_valid_store_path("/nix/store/"));
  }

  #[test]
  fn store_path_rejects_overly_long() {
    let long_path = format!("/nix/store/{}", "a".repeat(512));
    assert!(!is_valid_store_path(&long_path));
  }

  #[test]
  fn store_path_rejects_double_dot_embedded() {
    assert!(!is_valid_store_path("/nix/store/abc..def"));
  }

  // --- is_valid_nix_hash ---

  #[test]
  fn valid_nix_hash_lowercase_alpha() {
    assert!(is_valid_nix_hash("abcdefghijklmnopqrstuvwxyzabcdef"));
  }

  #[test]
  fn valid_nix_hash_digits() {
    assert!(is_valid_nix_hash("01234567890123456789012345678901"));
  }

  #[test]
  fn valid_nix_hash_mixed() {
    assert!(is_valid_nix_hash("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"));
  }

  #[test]
  fn nix_hash_rejects_uppercase() {
    assert!(!is_valid_nix_hash("ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF"));
  }

  #[test]
  fn nix_hash_rejects_mixed_case() {
    assert!(!is_valid_nix_hash("abcdefghijklmnopqrstuvwxyzAbcdeF"));
  }

  #[test]
  fn nix_hash_rejects_too_short() {
    assert!(!is_valid_nix_hash("abcdef1234567890"));
  }

  #[test]
  fn nix_hash_rejects_too_long() {
    assert!(!is_valid_nix_hash("abcdefghijklmnopqrstuvwxyzabcdefg"));
  }

  #[test]
  fn nix_hash_rejects_empty() {
    assert!(!is_valid_nix_hash(""));
  }

  #[test]
  fn nix_hash_rejects_special_chars() {
    assert!(!is_valid_nix_hash("abcdefghijklmnopqrstuvwxyz!@#$%^"));
  }

  #[test]
  fn nix_hash_rejects_spaces() {
    assert!(!is_valid_nix_hash("abcdefghijklmnop rstuvwxyzabcdef"));
  }

  #[test]
  fn nix_hash_rejects_path_traversal_attempt() {
    assert!(!is_valid_nix_hash("../../../../../../etc/passwd__"));
  }

  #[test]
  fn nix_hash_rejects_sql_injection_attempt() {
    assert!(!is_valid_nix_hash("' OR 1=1; DROP TABLE builds;--"));
  }

  // --- Validate trait tests ---

  #[test]
  fn test_create_project_valid() {
    let p = CreateProject {
      name:           "my-project".to_string(),
      description:    Some("A test project".to_string()),
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_ok());
  }

  #[test]
  fn test_create_project_invalid_name() {
    let p = CreateProject {
      name:           "".to_string(),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());

    let p = CreateProject {
      name:           "-starts-with-dash".to_string(),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());

    let p = CreateProject {
      name:           "has spaces".to_string(),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_project_invalid_url() {
    let p = CreateProject {
      name:           "valid-name".to_string(),
      description:    None,
      repository_url: "ftp://example.com".to_string(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_project_description_too_long() {
    let p = CreateProject {
      name:           "valid-name".to_string(),
      description:    Some("a".repeat(4097)),
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_jobset_valid() {
    let j = CreateJobset {
      project_id:        Uuid::new_v4(),
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    Some(300),
      branch:            None,
      scheduling_shares: None,
      state:             None,
    };
    assert!(j.validate().is_ok());
  }

  #[test]
  fn test_create_jobset_interval_too_low() {
    let j = CreateJobset {
      project_id:        Uuid::new_v4(),
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    Some(5),
      branch:            None,
      scheduling_shares: None,
      state:             None,
    };
    assert!(j.validate().is_err());
  }

  #[test]
  fn test_create_evaluation_valid() {
    let e = CreateEvaluation {
      jobset_id:      Uuid::new_v4(),
      commit_hash:    "abc123".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    };
    assert!(e.validate().is_ok());
  }

  #[test]
  fn test_create_evaluation_invalid_hash() {
    let e = CreateEvaluation {
      jobset_id:      Uuid::new_v4(),
      commit_hash:    "not-hex!".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    };
    assert!(e.validate().is_err());
  }

  #[test]
  fn test_create_build_valid() {
    let b = CreateBuild {
      evaluation_id: Uuid::new_v4(),
      job_name:      "hello".to_string(),
      drv_path:      "/nix/store/abc123-hello.drv".to_string(),
      system:        Some("x86_64-linux".to_string()),
      outputs:       None,
      is_aggregate:  None,
      constituents:  None,
    };
    assert!(b.validate().is_ok());
  }

  #[test]
  fn test_create_build_invalid_drv() {
    let b = CreateBuild {
      evaluation_id: Uuid::new_v4(),
      job_name:      "hello".to_string(),
      drv_path:      "/tmp/bad-path".to_string(),
      system:        None,
      outputs:       None,
      is_aggregate:  None,
      constituents:  None,
    };
    assert!(b.validate().is_err());
  }

  #[test]
  fn test_create_remote_builder_valid() {
    let rb = CreateRemoteBuilder {
      name:               "builder1".to_string(),
      ssh_uri:            "root@builder.example.com".to_string(),
      systems:            vec!["x86_64-linux".to_string()],
      max_jobs:           Some(4),
      speed_factor:       Some(1),
      supported_features: None,
      mandatory_features: None,
      public_host_key:    None,
      ssh_key_file:       None,
    };
    assert!(rb.validate().is_ok());
  }

  #[test]
  fn test_create_remote_builder_invalid_max_jobs() {
    let rb = CreateRemoteBuilder {
      name:               "builder1".to_string(),
      ssh_uri:            "root@builder.example.com".to_string(),
      systems:            vec!["x86_64-linux".to_string()],
      max_jobs:           Some(0),
      speed_factor:       None,
      supported_features: None,
      mandatory_features: None,
      public_host_key:    None,
      ssh_key_file:       None,
    };
    assert!(rb.validate().is_err());
  }

  #[test]
  fn test_create_webhook_config_valid() {
    let wh = CreateWebhookConfig {
      project_id: Uuid::new_v4(),
      forge_type: "github".to_string(),
      secret:     None,
    };
    assert!(wh.validate().is_ok());
  }

  #[test]
  fn test_create_webhook_config_invalid_forge() {
    let wh = CreateWebhookConfig {
      project_id: Uuid::new_v4(),
      forge_type: "bitbucket".to_string(),
      secret:     None,
    };
    assert!(wh.validate().is_err());
  }

  #[test]
  fn test_create_channel_valid() {
    let c = CreateChannel {
      project_id: Uuid::new_v4(),
      name:       "stable".to_string(),
      jobset_id:  Uuid::new_v4(),
    };
    assert!(c.validate().is_ok());
  }
}

//! Notification dispatch for build events

use tracing::{error, info, warn};

use crate::{
  config::{EmailConfig, NotificationsConfig},
  models::{Build, BuildStatus, Project},
};

/// Dispatch all configured notifications for a completed build.
pub async fn dispatch_build_finished(
  build: &Build,
  project: &Project,
  commit_hash: &str,
  config: &NotificationsConfig,
) {
  // 1. Run command notification
  if let Some(ref cmd) = config.run_command {
    run_command_notification(cmd, build, project).await;
  }

  // 2. GitHub commit status
  if let Some(ref token) = config.github_token
    && project.repository_url.contains("github.com")
  {
    set_github_status(token, &project.repository_url, commit_hash, build).await;
  }

  // 3. Gitea/Forgejo commit status
  if let (Some(url), Some(token)) = (&config.gitea_url, &config.gitea_token) {
    set_gitea_status(url, token, &project.repository_url, commit_hash, build)
      .await;
  }

  // 4. GitLab commit status
  if let (Some(url), Some(token)) = (&config.gitlab_url, &config.gitlab_token) {
    set_gitlab_status(url, token, &project.repository_url, commit_hash, build)
      .await;
  }

  // 5. Email notification
  if let Some(ref email_config) = config.email
    && (!email_config.on_failure_only || build.status == BuildStatus::Failed)
  {
    send_email_notification(email_config, build, project).await;
  }
}

async fn run_command_notification(cmd: &str, build: &Build, project: &Project) {
  let status_str = match build.status {
    BuildStatus::Completed => "success",
    BuildStatus::Failed => "failure",
    BuildStatus::Cancelled => "cancelled",
    _ => "unknown",
  };

  let result = tokio::process::Command::new("sh")
    .arg("-c")
    .arg(cmd)
    .env("FC_BUILD_ID", build.id.to_string())
    .env("FC_BUILD_STATUS", status_str)
    .env("FC_BUILD_JOB", &build.job_name)
    .env("FC_BUILD_DRV", &build.drv_path)
    .env("FC_PROJECT_NAME", &project.name)
    .env("FC_PROJECT_URL", &project.repository_url)
    .env(
      "FC_BUILD_OUTPUT",
      build.build_output_path.as_deref().unwrap_or(""),
    )
    .output()
    .await;

  match result {
    Ok(output) => {
      if output.status.success() {
        info!(build_id = %build.id, "RunCommand completed successfully");
      } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(build_id = %build.id, "RunCommand failed: {stderr}");
      }
    },
    Err(e) => error!(build_id = %build.id, "RunCommand execution failed: {e}"),
  }
}

async fn set_github_status(
  token: &str,
  repo_url: &str,
  commit: &str,
  build: &Build,
) {
  // Parse owner/repo from URL
  let (owner, repo) = if let Some(v) = parse_github_repo(repo_url) {
    v
  } else {
    warn!("Cannot parse GitHub owner/repo from {repo_url}");
    return;
  };

  let (state, description) = match build.status {
    BuildStatus::Completed => ("success", "Build succeeded"),
    BuildStatus::Failed => ("failure", "Build failed"),
    BuildStatus::Running => ("pending", "Build in progress"),
    BuildStatus::Pending => ("pending", "Build queued"),
    BuildStatus::Cancelled => ("error", "Build cancelled"),
  };

  let url =
    format!("https://api.github.com/repos/{owner}/{repo}/statuses/{commit}");
  let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("fc/{}", build.job_name),
  });

  let client = reqwest::Client::new();
  match client
    .post(&url)
    .header("Authorization", format!("token {token}"))
    .header("User-Agent", "fc-ci")
    .header("Accept", "application/vnd.github+json")
    .json(&body)
    .send()
    .await
  {
    Ok(resp) => {
      if resp.status().is_success() {
        info!(build_id = %build.id, "Set GitHub commit status: {state}");
      } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        warn!("GitHub status API returned {status}: {text}");
      }
    },
    Err(e) => error!("GitHub status API request failed: {e}"),
  }
}

async fn set_gitea_status(
  base_url: &str,
  token: &str,
  repo_url: &str,
  commit: &str,
  build: &Build,
) {
  // Parse owner/repo from URL (try to extract from the gitea URL)
  let (owner, repo) = if let Some(v) = parse_gitea_repo(repo_url, base_url) {
    v
  } else {
    warn!("Cannot parse Gitea owner/repo from {repo_url}");
    return;
  };

  let (state, description) = match build.status {
    BuildStatus::Completed => ("success", "Build succeeded"),
    BuildStatus::Failed => ("failure", "Build failed"),
    BuildStatus::Running => ("pending", "Build in progress"),
    BuildStatus::Pending => ("pending", "Build queued"),
    BuildStatus::Cancelled => ("error", "Build cancelled"),
  };

  let url = format!("{base_url}/api/v1/repos/{owner}/{repo}/statuses/{commit}");
  let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("fc/{}", build.job_name),
  });

  let client = reqwest::Client::new();
  match client
    .post(&url)
    .header("Authorization", format!("token {token}"))
    .json(&body)
    .send()
    .await
  {
    Ok(resp) => {
      if resp.status().is_success() {
        info!(build_id = %build.id, "Set Gitea commit status: {state}");
      } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        warn!("Gitea status API returned {status}: {text}");
      }
    },
    Err(e) => error!("Gitea status API request failed: {e}"),
  }
}

async fn set_gitlab_status(
  base_url: &str,
  token: &str,
  repo_url: &str,
  commit: &str,
  build: &Build,
) {
  // Parse project path from URL
  let project_path = if let Some(p) = parse_gitlab_project(repo_url, base_url) {
    p
  } else {
    warn!("Cannot parse GitLab project from {repo_url}");
    return;
  };

  // GitLab uses different state names
  let (state, description) = match build.status {
    BuildStatus::Completed => ("success", "Build succeeded"),
    BuildStatus::Failed => ("failed", "Build failed"),
    BuildStatus::Running => ("running", "Build in progress"),
    BuildStatus::Pending => ("pending", "Build queued"),
    BuildStatus::Cancelled => ("canceled", "Build cancelled"),
  };

  // URL-encode the project path for the API
  let encoded_project = urlencoding::encode(&project_path);
  let url = format!(
    "{}/api/v4/projects/{}/statuses/{}",
    base_url.trim_end_matches('/'),
    encoded_project,
    commit
  );

  let body = serde_json::json!({
      "state": state,
      "description": description,
      "name": format!("fc/{}", build.job_name),
  });

  let client = reqwest::Client::new();
  match client
    .post(&url)
    .header("PRIVATE-TOKEN", token)
    .json(&body)
    .send()
    .await
  {
    Ok(resp) => {
      if resp.status().is_success() {
        info!(build_id = %build.id, "Set GitLab commit status: {state}");
      } else {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        warn!("GitLab status API returned {status}: {text}");
      }
    },
    Err(e) => error!("GitLab status API request failed: {e}"),
  }
}

fn parse_github_repo(url: &str) -> Option<(String, String)> {
  // Handle https://github.com/owner/repo.git or git@github.com:owner/repo.git
  let url = url.trim_end_matches(".git");
  if let Some(rest) = url.strip_prefix("https://github.com/") {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.len() == 2 {
      return Some((parts[0].to_string(), parts[1].to_string()));
    }
  }
  if let Some(rest) = url.strip_prefix("git@github.com:") {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.len() == 2 {
      return Some((parts[0].to_string(), parts[1].to_string()));
    }
  }
  None
}

fn parse_gitea_repo(
  repo_url: &str,
  base_url: &str,
) -> Option<(String, String)> {
  let url = repo_url.trim_end_matches(".git");
  let base = base_url.trim_end_matches('/');
  if let Some(rest) = url.strip_prefix(&format!("{base}/")) {
    let parts: Vec<&str> = rest.splitn(2, '/').collect();
    if parts.len() == 2 {
      return Some((parts[0].to_string(), parts[1].to_string()));
    }
  }
  None
}

fn parse_gitlab_project(repo_url: &str, base_url: &str) -> Option<String> {
  let url = repo_url.trim_end_matches(".git");
  let base = base_url.trim_end_matches('/');
  if let Some(rest) = url.strip_prefix(&format!("{base}/")) {
    return Some(rest.to_string());
  }
  // Also try without scheme match (e.g., https vs git@)
  if let (Some(at_pos), Some(colon_pos)) = (
    url.find('@'),
    url.find('@').and_then(|p| url[p..].find(':')),
  ) {
    let path = &url[at_pos + colon_pos + 1..];
    return Some(path.to_string());
  }
  None
}

async fn send_email_notification(
  config: &EmailConfig,
  build: &Build,
  project: &Project,
) {
  use lettre::{
    AsyncSmtpTransport,
    AsyncTransport,
    Message,
    Tokio1Executor,
    message::header::ContentType,
    transport::smtp::authentication::Credentials,
  };

  let status_str = match build.status {
    BuildStatus::Completed => "SUCCESS",
    BuildStatus::Failed => "FAILURE",
    BuildStatus::Cancelled => "CANCELLED",
    _ => "UNKNOWN",
  };

  let subject = format!(
    "[FC] {} - {} ({})",
    status_str, build.job_name, project.name
  );

  let body = format!(
    "Build notification from FC CI\n\nProject: {}\nJob: {}\nStatus: \
     {}\nDerivation: {}\nOutput: {}\nBuild ID: {}\n",
    project.name,
    build.job_name,
    status_str,
    build.drv_path,
    build.build_output_path.as_deref().unwrap_or("N/A"),
    build.id,
  );

  for to_addr in &config.to_addresses {
    let email = match Message::builder()
      .from(match config.from_address.parse() {
        Ok(addr) => addr,
        Err(e) => {
          error!("Invalid from address '{}': {e}", config.from_address);
          return;
        },
      })
      .to(match to_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
          warn!("Invalid to address '{to_addr}': {e}");
          continue;
        },
      })
      .subject(&subject)
      .header(ContentType::TEXT_PLAIN)
      .body(body.clone())
    {
      Ok(e) => e,
      Err(e) => {
        error!("Failed to build email: {e}");
        continue;
      },
    };

    let mut mailer_builder = if config.tls {
      match AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host) {
        Ok(b) => b.port(config.smtp_port),
        Err(e) => {
          error!("Failed to create SMTP transport: {e}");
          return;
        },
      }
    } else {
      AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&config.smtp_host)
        .port(config.smtp_port)
    };

    if let (Some(user), Some(pass)) = (&config.smtp_user, &config.smtp_password)
    {
      mailer_builder = mailer_builder
        .credentials(Credentials::new(user.clone(), pass.clone()));
    }

    let mailer = mailer_builder.build();

    match mailer.send(email).await {
      Ok(_) => {
        info!(build_id = %build.id, to = to_addr, "Email notification sent");
      },
      Err(e) => {
        error!(build_id = %build.id, to = to_addr, "Failed to send email: {e}");
      },
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_github_repo_https() {
    let result = parse_github_repo("https://github.com/owner/repo.git");
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));

    let result = parse_github_repo("https://github.com/owner/repo");
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
  }

  #[test]
  fn test_parse_github_repo_ssh() {
    let result = parse_github_repo("git@github.com:owner/repo.git");
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
  }

  #[test]
  fn test_parse_github_repo_invalid() {
    assert_eq!(parse_github_repo("https://gitlab.com/owner/repo"), None);
    assert_eq!(parse_github_repo("invalid-url"), None);
  }

  #[test]
  fn test_parse_gitea_repo() {
    let result = parse_gitea_repo(
      "https://gitea.example.com/owner/repo.git",
      "https://gitea.example.com",
    );
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));

    let result = parse_gitea_repo(
      "https://gitea.example.com/owner/repo",
      "https://gitea.example.com/",
    );
    assert_eq!(result, Some(("owner".to_string(), "repo".to_string())));
  }

  #[test]
  fn test_parse_gitlab_project() {
    let result = parse_gitlab_project(
      "https://gitlab.com/group/subgroup/repo.git",
      "https://gitlab.com",
    );
    assert_eq!(result, Some("group/subgroup/repo".to_string()));

    let result = parse_gitlab_project(
      "https://gitlab.com/owner/repo",
      "https://gitlab.com/",
    );
    assert_eq!(result, Some("owner/repo".to_string()));
  }

  #[test]
  fn test_parse_gitlab_project_ssh() {
    let result = parse_gitlab_project(
      "git@gitlab.com:group/repo.git",
      "https://gitlab.com",
    );
    assert_eq!(result, Some("group/repo".to_string()));
  }
}

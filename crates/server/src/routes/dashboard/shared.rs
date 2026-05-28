//! View models, formatting helpers, status-badge mappings, and per-request
//! auth helpers shared across all dashboard handlers. Everything here is
//! `pub(super)` so sibling modules (auth, admin, pages, ...) can use them
//! without re-exporting them at the dashboard module's external surface.
//!
//! `allow(dead_code)` is needed because askama template fields are read
//! via the derive-generated `render()` impl, which rustc does not detect.

#![allow(dead_code)]

use axum::http::Extensions;
use circus_common::models::{
  ApiKey,
  Build,
  BuildStatus,
  Evaluation,
  EvaluationStatus,
  User,
};
use uuid::Uuid;

// View models (pre-formatted for templates)

pub(super) struct BuildView {
  pub(super) id:            Uuid,
  pub(super) job_name:      String,
  pub(super) status_text:   String,
  pub(super) status_class:  String,
  pub(super) system:        String,
  pub(super) created_at:    String,
  pub(super) started_at:    String,
  pub(super) completed_at:  String,
  pub(super) duration:      String,
  pub(super) priority:      i32,
  pub(super) is_aggregate:  bool,
  pub(super) signed:        bool,
  pub(super) drv_path:      String,
  pub(super) output_path:   String,
  pub(super) error_message: String,
  pub(super) log_url:       String,
}

/// Queue page build info with elapsed time and builder details
pub(super) struct QueueBuildView {
  pub(super) id:           Uuid,
  pub(super) job_name:     String,
  pub(super) system:       String,
  pub(super) created_at:   String,
  pub(super) started_at:   String,
  pub(super) elapsed:      String,
  pub(super) priority:     i32,
  pub(super) builder_name: Option<String>,
  pub(super) queue_pos:    i64,
}

pub(super) struct EvalView {
  pub(super) id:            Uuid,
  pub(super) commit_hash:   String,
  pub(super) commit_short:  String,
  pub(super) status_text:   String,
  pub(super) status_class:  String,
  pub(super) time:          String,
  pub(super) error_message: Option<String>,
  pub(super) jobset_name:   String,
  pub(super) project_name:  String,
}

pub(super) struct EvalSummaryView {
  pub(super) id:           Uuid,
  pub(super) commit_short: String,
  pub(super) status_text:  String,
  pub(super) status_class: String,
  pub(super) time:         String,
  pub(super) succeeded:    i64,
  pub(super) failed:       i64,
  pub(super) pending:      i64,
}

pub(super) struct ProjectSummaryView {
  pub(super) id:               Uuid,
  pub(super) name:             String,
  pub(super) jobset_count:     i64,
  pub(super) last_eval_status: String,
  pub(super) last_eval_class:  String,
  pub(super) last_eval_time:   String,
}

pub(super) struct ApiKeyView {
  pub(super) id:           Uuid,
  pub(super) name:         String,
  pub(super) role:         String,
  pub(super) created_at:   String,
  pub(super) last_used_at: String,
}

pub(super) struct UserView {
  pub(super) id:            Uuid,
  pub(super) username:      String,
  pub(super) email:         String,
  pub(super) role:          String,
  pub(super) user_type:     String,
  pub(super) enabled:       bool,
  pub(super) last_login_at: String,
}

pub(super) struct StarredJobView {
  pub(super) id:              Uuid,
  pub(super) project_id:      Uuid,
  pub(super) project_name:    String,
  pub(super) jobset_id:       Option<Uuid>,
  pub(super) jobset_name:     String,
  pub(super) job_name:        String,
  pub(super) status_text:     String,
  pub(super) status_class:    String,
  pub(super) latest_build_id: Option<Uuid>,
}

pub(super) fn format_duration(
  started: Option<&chrono::DateTime<chrono::Utc>>,
  completed: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
  match (started, completed) {
    (Some(s), Some(c)) => {
      let secs = (*c - *s).num_seconds();
      if secs < 0 {
        return String::new();
      }
      let mins = secs / 60;
      let rem = secs % 60;
      if mins > 0 {
        format!("{mins}m {rem}s")
      } else {
        format!("{rem}s")
      }
    },
    _ => String::new(),
  }
}

pub(super) fn build_view(b: &Build) -> BuildView {
  let (text, class) = status_badge(&b.status);
  BuildView {
    id:            b.id,
    job_name:      b.job_name.clone(),
    status_text:   text,
    status_class:  class,
    system:        b.system.clone().unwrap_or_else(|| "-".to_string()),
    created_at:    b.created_at.format("%Y-%m-%d %H:%M").to_string(),
    started_at:    b
      .started_at
      .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default(),
    completed_at:  b
      .completed_at
      .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default(),
    duration:      format_duration(
      b.started_at.as_ref(),
      b.completed_at.as_ref(),
    ),
    priority:      b.priority,
    is_aggregate:  b.is_aggregate,
    signed:        b.signed,
    drv_path:      b.drv_path.clone(),
    output_path:   b.build_output_path.clone().unwrap_or_default(),
    error_message: b.error_message.clone().unwrap_or_default(),
    log_url:       b.log_url.clone().unwrap_or_default(),
  }
}

pub(super) fn eval_view(e: &Evaluation) -> EvalView {
  let (text, class) = eval_badge(&e.status);
  let short = if e.commit_hash.len() > 12 {
    e.commit_hash[..12].to_string()
  } else {
    e.commit_hash.clone()
  };
  EvalView {
    id:            e.id,
    commit_hash:   e.commit_hash.clone(),
    commit_short:  short,
    status_text:   text,
    status_class:  class,
    time:          e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
    error_message: e.error_message.clone(),
    jobset_name:   String::new(),
    project_name:  String::new(),
  }
}

pub(super) fn eval_view_with_context(
  e: &Evaluation,
  jobset_name: &str,
  project_name: &str,
) -> EvalView {
  let mut v = eval_view(e);
  v.jobset_name = jobset_name.to_string();
  v.project_name = project_name.to_string();
  v
}

pub(super) fn status_badge(s: &BuildStatus) -> (String, String) {
  match s {
    BuildStatus::Succeeded => ("Succeeded".into(), "succeeded".into()),
    BuildStatus::Failed => ("Failed".into(), "failed".into()),
    BuildStatus::Running => ("Running".into(), "running".into()),
    BuildStatus::Pending => ("Pending".into(), "pending".into()),
    BuildStatus::Cancelled => ("Cancelled".into(), "cancelled".into()),
    BuildStatus::DependencyFailed => {
      ("Dependency Failed".into(), "failed".into())
    },
    BuildStatus::Aborted => ("Aborted".into(), "aborted".into()),
    BuildStatus::FailedWithOutput => {
      ("Failed w/ Output".into(), "failed".into())
    },
    BuildStatus::Timeout => ("Timeout".into(), "failed".into()),
    BuildStatus::CachedFailure => ("Cached Failure".into(), "failed".into()),
    BuildStatus::UnsupportedSystem => {
      ("Unsupported System".into(), "skipped".into())
    },
    BuildStatus::LogLimitExceeded => ("Log Limit".into(), "failed".into()),
    BuildStatus::NarSizeLimitExceeded => {
      ("NAR Size Limit".into(), "failed".into())
    },
    BuildStatus::NonDeterministic => {
      ("Non-deterministic".into(), "failed".into())
    },
  }
}

pub(super) fn eval_badge(s: &EvaluationStatus) -> (String, String) {
  match s {
    EvaluationStatus::Completed => ("Completed".into(), "completed".into()),
    EvaluationStatus::Failed => ("Failed".into(), "failed".into()),
    EvaluationStatus::Running => ("Running".into(), "running".into()),
    EvaluationStatus::Pending => ("Pending".into(), "pending".into()),
  }
}

pub(super) fn is_admin(extensions: &Extensions) -> bool {
  if let Some(user) = extensions.get::<User>() {
    return user.role == "admin";
  }
  extensions
    .get::<ApiKey>()
    .is_some_and(|k| k.role == "admin")
}

pub(super) fn auth_name(extensions: &Extensions) -> String {
  if let Some(user) = extensions.get::<User>() {
    return user.username.clone();
  }
  extensions
    .get::<ApiKey>()
    .map(|k| k.name.clone())
    .unwrap_or_default()
}

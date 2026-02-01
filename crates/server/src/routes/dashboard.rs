use askama::Template;
use axum::{
  Form,
  Router,
  extract::{Path, Query, State},
  http::Extensions,
  response::{Html, IntoResponse, Redirect, Response},
  routing::get,
};
use fc_common::models::*;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::state::AppState;

// --- View models (pre-formatted for templates) ---

struct BuildView {
  id:            Uuid,
  job_name:      String,
  status_text:   String,
  status_class:  String,
  system:        String,
  created_at:    String,
  started_at:    String,
  completed_at:  String,
  duration:      String,
  priority:      i32,
  is_aggregate:  bool,
  signed:        bool,
  drv_path:      String,
  output_path:   String,
  error_message: String,
  log_url:       String,
}

struct EvalView {
  id:            Uuid,
  commit_hash:   String,
  commit_short:  String,
  status_text:   String,
  status_class:  String,
  time:          String,
  error_message: Option<String>,
  jobset_name:   String,
  project_name:  String,
}

struct EvalSummaryView {
  id:           Uuid,
  commit_short: String,
  status_text:  String,
  status_class: String,
  time:         String,
  succeeded:    i64,
  failed:       i64,
  pending:      i64,
}

struct ProjectSummaryView {
  id:               Uuid,
  name:             String,
  jobset_count:     i64,
  last_eval_status: String,
  last_eval_class:  String,
  last_eval_time:   String,
}

struct ApiKeyView {
  id:           Uuid,
  name:         String,
  role:         String,
  created_at:   String,
  last_used_at: String,
}

fn format_duration(
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

fn build_view(b: &Build) -> BuildView {
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

fn eval_view(e: &Evaluation) -> EvalView {
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

fn eval_view_with_context(
  e: &Evaluation,
  jobset_name: &str,
  project_name: &str,
) -> EvalView {
  let mut v = eval_view(e);
  v.jobset_name = jobset_name.to_string();
  v.project_name = project_name.to_string();
  v
}

fn status_badge(s: &BuildStatus) -> (String, String) {
  match s {
    BuildStatus::Completed => ("Completed".into(), "completed".into()),
    BuildStatus::Failed => ("Failed".into(), "failed".into()),
    BuildStatus::Running => ("Running".into(), "running".into()),
    BuildStatus::Pending => ("Pending".into(), "pending".into()),
    BuildStatus::Cancelled => ("Cancelled".into(), "cancelled".into()),
  }
}

fn eval_badge(s: &EvaluationStatus) -> (String, String) {
  match s {
    EvaluationStatus::Completed => ("Completed".into(), "completed".into()),
    EvaluationStatus::Failed => ("Failed".into(), "failed".into()),
    EvaluationStatus::Running => ("Running".into(), "running".into()),
    EvaluationStatus::Pending => ("Pending".into(), "pending".into()),
  }
}

fn is_admin(extensions: &Extensions) -> bool {
  extensions
    .get::<ApiKey>()
    .map(|k| k.role == "admin")
    .unwrap_or(false)
}

fn auth_name(extensions: &Extensions) -> String {
  extensions
    .get::<ApiKey>()
    .map(|k| k.name.clone())
    .unwrap_or_default()
}

// --- Templates ---

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
  total_builds:     i64,
  completed_builds: i64,
  failed_builds:    i64,
  running_builds:   i64,
  pending_builds:   i64,
  recent_builds:    Vec<BuildView>,
  recent_evals:     Vec<EvalView>,
  projects:         Vec<ProjectSummaryView>,
  is_admin:         bool,
  auth_name:        String,
}

#[derive(Template)]
#[template(path = "projects.html")]
struct ProjectsTemplate {
  projects:    Vec<Project>,
  limit:       i64,
  has_prev:    bool,
  has_next:    bool,
  prev_offset: i64,
  next_offset: i64,
  page:        i64,
  total_pages: i64,
  is_admin:    bool,
  auth_name:   String,
}

#[derive(Template)]
#[template(path = "project.html")]
struct ProjectTemplate {
  project:      Project,
  jobsets:      Vec<Jobset>,
  recent_evals: Vec<EvalView>,
  is_admin:     bool,
  auth_name:    String,
}

#[derive(Template)]
#[template(path = "jobset.html")]
struct JobsetTemplate {
  project:        Project,
  jobset:         Jobset,
  eval_summaries: Vec<EvalSummaryView>,
}

#[derive(Template)]
#[template(path = "evaluations.html")]
struct EvaluationsTemplate {
  evals:       Vec<EvalView>,
  limit:       i64,
  has_prev:    bool,
  has_next:    bool,
  prev_offset: i64,
  next_offset: i64,
  page:        i64,
  total_pages: i64,
}

#[derive(Template)]
#[template(path = "evaluation.html")]
struct EvaluationTemplate {
  eval:            EvalView,
  builds:          Vec<BuildView>,
  project_name:    String,
  project_id:      Uuid,
  jobset_name:     String,
  jobset_id:       Uuid,
  succeeded_count: i64,
  failed_count:    i64,
  running_count:   i64,
  pending_count:   i64,
}

#[derive(Template)]
#[template(path = "builds.html")]
struct BuildsTemplate {
  builds:        Vec<BuildView>,
  limit:         i64,
  has_prev:      bool,
  has_next:      bool,
  prev_offset:   i64,
  next_offset:   i64,
  page:          i64,
  total_pages:   i64,
  filter_status: String,
  filter_system: String,
  filter_job:    String,
}

#[derive(Template)]
#[template(path = "build.html")]
struct BuildTemplate {
  build:             BuildView,
  steps:             Vec<BuildStep>,
  products:          Vec<BuildProduct>,
  eval_id:           Uuid,
  eval_commit_short: String,
  jobset_id:         Uuid,
  jobset_name:       String,
  project_id:        Uuid,
  project_name:      String,
}

#[derive(Template)]
#[template(path = "queue.html")]
struct QueueTemplate {
  pending_builds: Vec<BuildView>,
  running_builds: Vec<BuildView>,
  pending_count:  i64,
  running_count:  i64,
}

#[derive(Template)]
#[template(path = "channels.html")]
struct ChannelsTemplate {
  channels: Vec<Channel>,
}

#[derive(Template)]
#[template(path = "admin.html")]
struct AdminTemplate {
  status:    SystemStatus,
  builders:  Vec<RemoteBuilder>,
  api_keys:  Vec<ApiKeyView>,
  is_admin:  bool,
  auth_name: String,
}

#[derive(Template)]
#[template(path = "project_setup.html")]
#[allow(dead_code)]
struct ProjectSetupTemplate {
  is_admin:  bool,
  auth_name: String,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
  error: Option<String>,
}

// --- Handlers ---

async fn home(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  let stats = fc_common::repo::builds::get_stats(&state.pool)
    .await
    .unwrap_or_default();
  let builds = fc_common::repo::builds::list_recent(&state.pool, 10)
    .await
    .unwrap_or_default();
  let evals =
    fc_common::repo::evaluations::list_filtered(&state.pool, None, None, 5, 0)
      .await
      .unwrap_or_default();

  // Fetch project summaries
  let all_projects = fc_common::repo::projects::list(&state.pool, 10, 0)
    .await
    .unwrap_or_default();
  let mut project_summaries = Vec::new();
  for p in &all_projects {
    let jobset_count =
      fc_common::repo::jobsets::count_for_project(&state.pool, p.id)
        .await
        .unwrap_or(0);
    let jobsets =
      fc_common::repo::jobsets::list_for_project(&state.pool, p.id, 100, 0)
        .await
        .unwrap_or_default();
    let mut last_eval: Option<Evaluation> = None;
    for js in &jobsets {
      let js_evals = fc_common::repo::evaluations::list_filtered(
        &state.pool,
        Some(js.id),
        None,
        1,
        0,
      )
      .await
      .unwrap_or_default();
      if let Some(e) = js_evals.into_iter().next()
        && last_eval
          .as_ref()
          .is_none_or(|le| e.evaluation_time > le.evaluation_time)
      {
        last_eval = Some(e);
      }
    }
    let (status, class, time) = match &last_eval {
      Some(e) => {
        let (t, c) = eval_badge(&e.status);
        (t, c, e.evaluation_time.format("%Y-%m-%d %H:%M").to_string())
      },
      None => ("-".into(), "pending".into(), "-".into()),
    };
    project_summaries.push(ProjectSummaryView {
      id: p.id,
      name: p.name.clone(),
      jobset_count,
      last_eval_status: status,
      last_eval_class: class,
      last_eval_time: time,
    });
  }

  let tmpl = HomeTemplate {
    total_builds:     stats.total_builds.unwrap_or(0),
    completed_builds: stats.completed_builds.unwrap_or(0),
    failed_builds:    stats.failed_builds.unwrap_or(0),
    running_builds:   stats.running_builds.unwrap_or(0),
    pending_builds:   stats.pending_builds.unwrap_or(0),
    recent_builds:    builds.iter().map(build_view).collect(),
    recent_evals:     evals.iter().map(eval_view).collect(),
    projects:         project_summaries,
    is_admin:         is_admin(&extensions),
    auth_name:        auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

#[derive(serde::Deserialize)]
struct PageParams {
  limit:  Option<i64>,
  offset: Option<i64>,
}

async fn projects_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).min(200).max(1);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = fc_common::repo::projects::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = fc_common::repo::projects::count(&state.pool)
    .await
    .unwrap_or(0);

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;
  let tmpl = ProjectsTemplate {
    projects: items,
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn project_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let project = match fc_common::repo::projects::get(&state.pool, id).await {
    Ok(p) => p,
    Err(_) => return Html("Project not found".to_string()),
  };
  let jobsets =
    fc_common::repo::jobsets::list_for_project(&state.pool, id, 100, 0)
      .await
      .unwrap_or_default();

  // Get evaluations for this project's jobsets
  let mut evals = Vec::new();
  for js in &jobsets {
    let mut js_evals = fc_common::repo::evaluations::list_filtered(
      &state.pool,
      Some(js.id),
      None,
      5,
      0,
    )
    .await
    .unwrap_or_default();
    evals.append(&mut js_evals);
  }
  evals.sort_by(|a, b| b.evaluation_time.cmp(&a.evaluation_time));
  evals.truncate(10);

  let tmpl = ProjectTemplate {
    project,
    jobsets,
    recent_evals: evals.iter().map(eval_view).collect(),
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn jobset_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Html<String> {
  let jobset = match fc_common::repo::jobsets::get(&state.pool, id).await {
    Ok(j) => j,
    Err(_) => return Html("Jobset not found".to_string()),
  };
  let project = match fc_common::repo::projects::get(
    &state.pool,
    jobset.project_id,
  )
  .await
  {
    Ok(p) => p,
    Err(_) => return Html("Project not found".to_string()),
  };

  let evals = fc_common::repo::evaluations::list_filtered(
    &state.pool,
    Some(id),
    None,
    20,
    0,
  )
  .await
  .unwrap_or_default();

  let mut summaries = Vec::new();
  for e in &evals {
    let (text, class) = eval_badge(&e.status);
    let short = if e.commit_hash.len() > 12 {
      e.commit_hash[..12].to_string()
    } else {
      e.commit_hash.clone()
    };
    let succeeded = fc_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("completed"),
      None,
      None,
    )
    .await
    .unwrap_or(0);
    let failed = fc_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("failed"),
      None,
      None,
    )
    .await
    .unwrap_or(0);
    let pending = fc_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("pending"),
      None,
      None,
    )
    .await
    .unwrap_or(0);

    summaries.push(EvalSummaryView {
      id: e.id,
      commit_short: short,
      status_text: text,
      status_class: class,
      time: e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
      succeeded,
      failed,
      pending,
    });
  }

  let tmpl = JobsetTemplate {
    project,
    jobset,
    eval_summaries: summaries,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn evaluations_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).min(200).max(1);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = fc_common::repo::evaluations::list_filtered(
    &state.pool,
    None,
    None,
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total =
    fc_common::repo::evaluations::count_filtered(&state.pool, None, None)
      .await
      .unwrap_or(0);

  // Enrich evaluations with jobset/project names
  let mut enriched = Vec::new();
  for e in &items {
    let (jname, pname) =
      match fc_common::repo::jobsets::get(&state.pool, e.jobset_id).await {
        Ok(js) => {
          let pname =
            fc_common::repo::projects::get(&state.pool, js.project_id)
              .await
              .map(|p| p.name)
              .unwrap_or_else(|_| "-".to_string());
          (js.name, pname)
        },
        Err(_) => ("-".to_string(), "-".to_string()),
      };
    enriched.push(eval_view_with_context(e, &jname, &pname));
  }

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;
  let tmpl = EvaluationsTemplate {
    evals: enriched,
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn evaluation_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Html<String> {
  let eval = match fc_common::repo::evaluations::get(&state.pool, id).await {
    Ok(e) => e,
    Err(_) => return Html("Evaluation not found".to_string()),
  };

  let jobset =
    match fc_common::repo::jobsets::get(&state.pool, eval.jobset_id).await {
      Ok(j) => j,
      Err(_) => return Html("Jobset not found".to_string()),
    };
  let project = match fc_common::repo::projects::get(
    &state.pool,
    jobset.project_id,
  )
  .await
  {
    Ok(p) => p,
    Err(_) => return Html("Project not found".to_string()),
  };

  let builds = fc_common::repo::builds::list_filtered(
    &state.pool,
    Some(id),
    None,
    None,
    None,
    200,
    0,
  )
  .await
  .unwrap_or_default();

  let succeeded = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("completed"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let failed = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("failed"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let running = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("running"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let pending = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("pending"),
    None,
    None,
  )
  .await
  .unwrap_or(0);

  let tmpl = EvaluationTemplate {
    eval:            eval_view(&eval),
    builds:          builds.iter().map(build_view).collect(),
    project_name:    project.name,
    project_id:      project.id,
    jobset_name:     jobset.name,
    jobset_id:       jobset.id,
    succeeded_count: succeeded,
    failed_count:    failed,
    running_count:   running,
    pending_count:   pending,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

#[derive(serde::Deserialize)]
struct BuildFilterParams {
  status:   Option<String>,
  system:   Option<String>,
  job_name: Option<String>,
  limit:    Option<i64>,
  offset:   Option<i64>,
}

async fn builds_page(
  State(state): State<AppState>,
  Query(params): Query<BuildFilterParams>,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).min(200).max(1);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = fc_common::repo::builds::list_filtered(
    &state.pool,
    None,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total = fc_common::repo::builds::count_filtered(
    &state.pool,
    None,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
  )
  .await
  .unwrap_or(0);

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;
  let tmpl = BuildsTemplate {
    builds: items.iter().map(build_view).collect(),
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    filter_status: params.status.unwrap_or_default(),
    filter_system: params.system.unwrap_or_default(),
    filter_job: params.job_name.unwrap_or_default(),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn build_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Html<String> {
  let build = match fc_common::repo::builds::get(&state.pool, id).await {
    Ok(b) => b,
    Err(_) => return Html("Build not found".to_string()),
  };

  let eval =
    match fc_common::repo::evaluations::get(&state.pool, build.evaluation_id)
      .await
    {
      Ok(e) => e,
      Err(_) => return Html("Evaluation not found".to_string()),
    };
  let jobset =
    match fc_common::repo::jobsets::get(&state.pool, eval.jobset_id).await {
      Ok(j) => j,
      Err(_) => return Html("Jobset not found".to_string()),
    };
  let project = match fc_common::repo::projects::get(
    &state.pool,
    jobset.project_id,
  )
  .await
  {
    Ok(p) => p,
    Err(_) => return Html("Project not found".to_string()),
  };

  let eval_commit_short = if eval.commit_hash.len() > 12 {
    eval.commit_hash[..12].to_string()
  } else {
    eval.commit_hash.clone()
  };

  let steps = fc_common::repo::build_steps::list_for_build(&state.pool, id)
    .await
    .unwrap_or_default();
  let products =
    fc_common::repo::build_products::list_for_build(&state.pool, id)
      .await
      .unwrap_or_default();

  let tmpl = BuildTemplate {
    build: build_view(&build),
    steps,
    products,
    eval_id: eval.id,
    eval_commit_short,
    jobset_id: jobset.id,
    jobset_name: jobset.name,
    project_id: project.id,
    project_name: project.name,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn queue_page(State(state): State<AppState>) -> Html<String> {
  let running = fc_common::repo::builds::list_filtered(
    &state.pool,
    None,
    Some("running"),
    None,
    None,
    100,
    0,
  )
  .await
  .unwrap_or_default();
  let pending = fc_common::repo::builds::list_filtered(
    &state.pool,
    None,
    Some("pending"),
    None,
    None,
    100,
    0,
  )
  .await
  .unwrap_or_default();

  let running_count = running.len() as i64;
  let pending_count = pending.len() as i64;

  let tmpl = QueueTemplate {
    running_builds: running.iter().map(build_view).collect(),
    pending_builds: pending.iter().map(build_view).collect(),
    running_count,
    pending_count,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn channels_page(State(state): State<AppState>) -> Html<String> {
  let channels = fc_common::repo::channels::list_all(&state.pool)
    .await
    .unwrap_or_default();

  let tmpl = ChannelsTemplate { channels };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn admin_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  let pool = &state.pool;

  let projects: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let jobsets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let evaluations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let stats = fc_common::repo::builds::get_stats(pool)
    .await
    .unwrap_or_default();
  let builders_count = fc_common::repo::remote_builders::count(pool)
    .await
    .unwrap_or(0);
  let channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

  let status = SystemStatus {
    projects_count:    projects.0,
    jobsets_count:     jobsets.0,
    evaluations_count: evaluations.0,
    builds_pending:    stats.pending_builds.unwrap_or(0),
    builds_running:    stats.running_builds.unwrap_or(0),
    builds_completed:  stats.completed_builds.unwrap_or(0),
    builds_failed:     stats.failed_builds.unwrap_or(0),
    remote_builders:   builders_count,
    channels_count:    channels.0,
  };
  let builders = fc_common::repo::remote_builders::list(pool)
    .await
    .unwrap_or_default();

  // Fetch API keys for admin view
  let keys = fc_common::repo::api_keys::list(pool)
    .await
    .unwrap_or_default();
  let api_keys: Vec<ApiKeyView> = keys
    .into_iter()
    .map(|k| {
      ApiKeyView {
        id:           k.id,
        name:         k.name,
        role:         k.role,
        created_at:   k.created_at.format("%Y-%m-%d %H:%M").to_string(),
        last_used_at: k
          .last_used_at
          .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
          .unwrap_or_else(|| "Never".to_string()),
      }
    })
    .collect();

  let tmpl = AdminTemplate {
    status,
    builders,
    api_keys,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// --- Setup Wizard ---

async fn project_setup_page(extensions: Extensions) -> Html<String> {
  let tmpl = ProjectSetupTemplate {
    is_admin:  is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// --- Login / Logout ---

async fn login_page() -> Html<String> {
  let tmpl = LoginTemplate { error: None };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

#[derive(serde::Deserialize)]
struct LoginForm {
  api_key: String,
}

async fn login_action(
  State(state): State<AppState>,
  Form(form): Form<LoginForm>,
) -> Response {
  let token = form.api_key.trim();
  if token.is_empty() {
    let tmpl = LoginTemplate {
      error: Some("API key is required".to_string()),
    };
    return Html(
      tmpl
        .render()
        .unwrap_or_else(|e| format!("Template error: {e}")),
    )
    .into_response();
  }

  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  let key_hash = hex::encode(hasher.finalize());

  match fc_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await {
    Ok(Some(api_key)) => {
      let session_id = Uuid::new_v4().to_string();
      state
        .sessions
        .insert(session_id.clone(), crate::state::SessionData {
          api_key,
          created_at: std::time::Instant::now(),
        });

      let cookie = format!(
        "fc_session={}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400",
        session_id
      );
      (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/"),
      )
        .into_response()
    },
    _ => {
      let tmpl = LoginTemplate {
        error: Some("Invalid API key".to_string()),
      };
      Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response()
    },
  }
}

async fn logout_action(
  State(state): State<AppState>,
  request: axum::extract::Request,
) -> Response {
  // Remove server-side session
  if let Some(cookie_header) = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
    && let Some(session_id) = cookie_header
      .split(';')
      .filter_map(|pair| {
        let pair = pair.trim();
        let (k, v) = pair.split_once('=')?;
        if k.trim() == "fc_session" {
          Some(v.trim().to_string())
        } else {
          None
        }
      })
      .next()
  {
    state.sessions.remove(&session_id);
  }

  let cookie = "fc_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
  (
    [(axum::http::header::SET_COOKIE, cookie.to_string())],
    Redirect::to("/"),
  )
    .into_response()
}

pub fn router(state: AppState) -> Router<AppState> {
  let _ = state; // used by middleware layer in mod.rs
  Router::new()
    .route("/login", get(login_page).post(login_action))
    .route("/logout", axum::routing::post(logout_action))
    .route("/", get(home))
    .route("/projects", get(projects_page))
    .route("/projects/new", get(project_setup_page))
    .route("/project/{id}", get(project_page))
    .route("/jobset/{id}", get(jobset_page))
    .route("/evaluations", get(evaluations_page))
    .route("/evaluation/{id}", get(evaluation_page))
    .route("/builds", get(builds_page))
    .route("/build/{id}", get(build_page))
    .route("/queue", get(queue_page))
    .route("/channels", get(channels_page))
    .route("/admin", get(admin_page))
}

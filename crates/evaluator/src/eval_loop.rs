use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use fc_common::{
  config::EvaluatorConfig,
  error::check_disk_space,
  models::{
    CreateBuild,
    CreateEvaluation,
    EvaluationStatus,
    JobsetInput,
    JobsetState,
  },
  repo,
};
use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn run(pool: PgPool, config: EvaluatorConfig) -> anyhow::Result<()> {
  let poll_interval = Duration::from_secs(config.poll_interval);
  let nix_timeout = Duration::from_secs(config.nix_timeout);
  let git_timeout = Duration::from_secs(config.git_timeout);

  loop {
    if let Err(e) = run_cycle(&pool, &config, nix_timeout, git_timeout).await {
      tracing::error!("Evaluation cycle failed: {e}");
    }
    tokio::time::sleep(poll_interval).await;
  }
}

async fn run_cycle(
  pool: &PgPool,
  config: &EvaluatorConfig,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> anyhow::Result<()> {
  let active = repo::jobsets::list_active(pool).await?;

  // Filter to jobsets that are due for evaluation based on their
  // check_interval and last_checked_at
  let now = Utc::now();
  let ready: Vec<_> = active
    .into_iter()
    .filter(|js| {
      match js.last_checked_at {
        Some(last) => {
          let elapsed = (now - last).num_seconds();
          elapsed >= i64::from(js.check_interval)
        },
        None => true, // Never checked, evaluate now
      }
    })
    .collect();

  tracing::info!("Found {} jobsets due for evaluation", ready.len());

  let max_concurrent = config.max_concurrent_evals;

  stream::iter(ready)
    .for_each_concurrent(max_concurrent, |jobset| {
      async move {
        if let Err(e) =
          evaluate_jobset(pool, &jobset, config, nix_timeout, git_timeout).await
        {
          tracing::error!(
              jobset_id = %jobset.id,
              jobset_name = %jobset.name,
              "Failed to evaluate jobset: {e}"
          );

          let msg = e.to_string().to_lowercase();
          if msg.contains("no space left on device")
            || msg.contains("disk full")
            || msg.contains("enospc")
            || msg.contains("cannot create")
            || msg.contains("sqlite")
          {
            tracing::error!(
              "DISK SPACE ISSUE DETECTED: Evaluation failed due to disk space \
               problems. Please free up space on the server:\n- Run \
               `nix-collect-garbage -d` to clean the Nix store\n- Clear \
               /tmp/fc-evaluator directory\n- Check build logs directory if \
               configured"
            );
          }
        }
      }
    })
    .await;

  Ok(())
}

async fn evaluate_jobset(
  pool: &PgPool,
  jobset: &fc_common::models::ActiveJobset,
  config: &EvaluatorConfig,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> anyhow::Result<()> {
  let url = jobset.repository_url.clone();
  let work_dir = config.work_dir.clone();
  let project_name = jobset.project_name.clone();
  let branch = jobset.branch.clone();

  tracing::info!(
      jobset = %jobset.name,
      project = %project_name,
      "Starting evaluation cycle"
  );

  match check_disk_space(&work_dir) {
    Ok(info) => {
      if info.is_critical() {
        tracing::error!(
          jobset = %jobset.name,
          "CRITICAL: Less than 1GB disk space available. {}",
          info.summary()
        );
      } else if info.is_low() {
        tracing::warn!(
          jobset = %jobset.name,
          "LOW: Less than 5GB disk space available. {}",
          info.summary()
        );
      }
    },
    Err(e) => {
      tracing::warn!(
        jobset = %jobset.name,
        "Disk space check failed: {}. Proceeding anyway...",
        e
      );
    },
  }

  // Clone/fetch in a blocking task (git2 is sync) with timeout
  let (repo_path, commit_hash) = tokio::time::timeout(
    git_timeout,
    tokio::task::spawn_blocking(move || {
      crate::git::clone_or_fetch(
        &url,
        &work_dir,
        &project_name,
        branch.as_deref(),
      )
    }),
  )
  .await
  .map_err(|_| {
    anyhow::anyhow!("Git operation timed out after {git_timeout:?}")
  })???;

  // Query jobset inputs
  let inputs = repo::jobset_inputs::list_for_jobset(pool, jobset.id)
    .await
    .unwrap_or_default();

  // Compute inputs hash for eval caching (commit + all input values/revisions)
  let inputs_hash = compute_inputs_hash(&commit_hash, &inputs);

  // Check if this exact combination was already evaluated (eval caching)
  if let Ok(Some(cached)) =
    repo::evaluations::get_by_inputs_hash(pool, jobset.id, &inputs_hash).await
  {
    tracing::debug!(
        jobset = %jobset.name,
        commit = %commit_hash,
        cached_eval = %cached.id,
        "Inputs unchanged (hash: {}), skipping evaluation",
        &inputs_hash[..16],
    );
    return Ok(());
  }

  // Also skip if commit hasn't changed (backward compat)
  if let Some(latest) = repo::evaluations::get_latest(pool, jobset.id).await?
    && latest.commit_hash == commit_hash
    && latest.inputs_hash.as_deref() == Some(&inputs_hash)
  {
    tracing::debug!(
        jobset = %jobset.name,
        commit = %commit_hash,
        "Already evaluated, skipping"
    );
    return Ok(());
  }

  tracing::info!(
      jobset = %jobset.name,
      commit = %commit_hash,
      "Starting evaluation"
  );

  // Create evaluation record
  let eval = repo::evaluations::create(pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    commit_hash.clone(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await?;

  // Mark as running and set inputs hash
  repo::evaluations::update_status(
    pool,
    eval.id,
    EvaluationStatus::Running,
    None,
  )
  .await?;
  let _ = repo::evaluations::set_inputs_hash(pool, eval.id, &inputs_hash).await;

  // Check for declarative config in repo
  check_declarative_config(pool, &repo_path, jobset.project_id).await;

  // Run nix evaluation
  match crate::nix::evaluate(
    &repo_path,
    &jobset.nix_expression,
    jobset.flake_mode,
    nix_timeout,
    config,
    &inputs,
  )
  .await
  {
    Ok(eval_result) => {
      tracing::info!(
          jobset = %jobset.name,
          count = eval_result.jobs.len(),
          errors = eval_result.error_count,
          "Evaluation discovered jobs"
      );

      // Create build records, tracking drv_path -> build_id for dependency
      // resolution
      let mut drv_to_build: HashMap<String, Uuid> = HashMap::new();
      let mut name_to_build: HashMap<String, Uuid> = HashMap::new();

      for job in &eval_result.jobs {
        let outputs_json = job
          .outputs
          .as_ref()
          .map(|o| serde_json::to_value(o).unwrap_or_default());
        let constituents_json = job
          .constituents
          .as_ref()
          .map(|c| serde_json::to_value(c).unwrap_or_default());
        let is_aggregate = job.constituents.is_some();

        let build = repo::builds::create(pool, CreateBuild {
          evaluation_id: eval.id,
          job_name:      job.name.clone(),
          drv_path:      job.drv_path.clone(),
          system:        job.system.clone(),
          outputs:       outputs_json,
          is_aggregate:  Some(is_aggregate),
          constituents:  constituents_json,
        })
        .await?;

        drv_to_build.insert(job.drv_path.clone(), build.id);
        name_to_build.insert(job.name.clone(), build.id);
      }

      // Resolve dependencies
      for job in &eval_result.jobs {
        let build_id = match drv_to_build.get(&job.drv_path) {
          Some(id) => *id,
          None => continue,
        };

        // Input derivation dependencies
        if let Some(ref input_drvs) = job.input_drvs {
          for dep_drv in input_drvs.keys() {
            if let Some(&dep_build_id) = drv_to_build.get(dep_drv)
              && dep_build_id != build_id
            {
              let _ =
                repo::build_dependencies::create(pool, build_id, dep_build_id)
                  .await;
            }
          }
        }

        // Aggregate constituent dependencies
        if let Some(ref constituents) = job.constituents {
          for constituent_name in constituents {
            if let Some(&dep_build_id) = name_to_build.get(constituent_name)
              && dep_build_id != build_id
            {
              let _ =
                repo::build_dependencies::create(pool, build_id, dep_build_id)
                  .await;
            }
          }
        }
      }

      repo::evaluations::update_status(
        pool,
        eval.id,
        EvaluationStatus::Completed,
        None,
      )
      .await?;
    },
    Err(e) => {
      let msg = e.to_string();
      tracing::error!(jobset = %jobset.name, "Evaluation failed: {msg}");
      repo::evaluations::update_status(
        pool,
        eval.id,
        EvaluationStatus::Failed,
        Some(&msg),
      )
      .await?;
    },
  }

  // Update last_checked_at timestamp for per-jobset interval tracking
  if let Err(e) = repo::jobsets::update_last_checked(pool, jobset.id).await {
    tracing::warn!(
      jobset = %jobset.name,
      "Failed to update last_checked_at: {e}"
    );
  }

  // Mark one-shot jobsets as complete (disabled) after evaluation
  if jobset.state == JobsetState::OneShot {
    tracing::info!(
      jobset = %jobset.name,
      "One-shot evaluation complete, disabling jobset"
    );
    if let Err(e) = repo::jobsets::mark_one_shot_complete(pool, jobset.id).await
    {
      tracing::error!(
        jobset = %jobset.name,
        "Failed to mark one-shot complete: {e}"
      );
    }
  }

  Ok(())
}

/// Compute a deterministic hash over the commit and all jobset inputs.
/// Used for evaluation caching — skip re-eval when inputs haven't changed.
fn compute_inputs_hash(commit_hash: &str, inputs: &[JobsetInput]) -> String {
  use sha2::{Digest, Sha256};

  let mut hasher = Sha256::new();
  hasher.update(commit_hash.as_bytes());

  // Sort inputs by name for deterministic hashing
  let mut sorted_inputs: Vec<&JobsetInput> = inputs.iter().collect();
  sorted_inputs.sort_by_key(|i| &i.name);

  for input in sorted_inputs {
    hasher.update(input.name.as_bytes());
    hasher.update(input.input_type.as_bytes());
    hasher.update(input.value.as_bytes());
    if let Some(ref rev) = input.revision {
      hasher.update(rev.as_bytes());
    }
  }

  hex::encode(hasher.finalize())
}

/// Check for declarative project config (.fc.toml or .fc/config.toml) in the
/// repo.
async fn check_declarative_config(
  pool: &PgPool,
  repo_path: &std::path::Path,
  project_id: Uuid,
) {
  let config_path = repo_path.join(".fc.toml");
  let alt_config_path = repo_path.join(".fc/config.toml");

  let path = if config_path.exists() {
    config_path
  } else if alt_config_path.exists() {
    alt_config_path
  } else {
    return;
  };

  let content = match std::fs::read_to_string(&path) {
    Ok(c) => c,
    Err(e) => {
      tracing::warn!(
        "Failed to read declarative config {}: {e}",
        path.display()
      );
      return;
    },
  };

  #[derive(serde::Deserialize)]
  struct DeclarativeConfig {
    jobsets: Option<Vec<DeclarativeJobset>>,
  }

  #[derive(serde::Deserialize)]
  struct DeclarativeJobset {
    name:           String,
    nix_expression: String,
    flake_mode:     Option<bool>,
    check_interval: Option<i32>,
    enabled:        Option<bool>,
  }

  let config: DeclarativeConfig = match toml::from_str(&content) {
    Ok(c) => c,
    Err(e) => {
      tracing::warn!("Failed to parse declarative config: {e}");
      return;
    },
  };

  if let Some(jobsets) = config.jobsets {
    for js in jobsets {
      let input = fc_common::models::CreateJobset {
        project_id,
        name: js.name,
        nix_expression: js.nix_expression,
        enabled: js.enabled,
        flake_mode: js.flake_mode,
        check_interval: js.check_interval,
        branch: None,
        scheduling_shares: None,
        state: None,
      };
      if let Err(e) = repo::jobsets::upsert(pool, input).await {
        tracing::warn!("Failed to upsert declarative jobset: {e}");
      }
    }
  }
}

use std::{path::PathBuf, sync::Arc, time::Duration};

use fc_common::{
  config::{
    CacheUploadConfig,
    GcConfig,
    LogConfig,
    NotificationsConfig,
    SigningConfig,
  },
  gc_roots::GcRoots,
  log_storage::LogStorage,
  models::{Build, BuildStatus, CreateBuildProduct, CreateBuildStep},
  repo,
};
use sqlx::PgPool;
use tokio::sync::Semaphore;

pub struct WorkerPool {
  semaphore:            Arc<Semaphore>,
  pool:                 PgPool,
  work_dir:             Arc<PathBuf>,
  build_timeout:        Duration,
  log_config:           Arc<LogConfig>,
  gc_config:            Arc<GcConfig>,
  notifications_config: Arc<NotificationsConfig>,
  signing_config:       Arc<SigningConfig>,
  cache_upload_config:  Arc<CacheUploadConfig>,
  drain_token:          tokio_util::sync::CancellationToken,
}

impl WorkerPool {
  pub fn new(
    db_pool: PgPool,
    workers: usize,
    work_dir: PathBuf,
    build_timeout: Duration,
    log_config: LogConfig,
    gc_config: GcConfig,
    notifications_config: NotificationsConfig,
    signing_config: SigningConfig,
    cache_upload_config: CacheUploadConfig,
  ) -> Self {
    Self {
      semaphore: Arc::new(Semaphore::new(workers)),
      pool: db_pool,
      work_dir: Arc::new(work_dir),
      build_timeout,
      log_config: Arc::new(log_config),
      gc_config: Arc::new(gc_config),
      notifications_config: Arc::new(notifications_config),
      signing_config: Arc::new(signing_config),
      cache_upload_config: Arc::new(cache_upload_config),
      drain_token: tokio_util::sync::CancellationToken::new(),
    }
  }

  /// Signal all workers to stop accepting new builds. In-flight builds will
  /// finish.
  pub fn drain(&self) {
    self.drain_token.cancel();
  }

  /// Wait until all in-flight builds complete (semaphore fully available).
  pub async fn wait_for_drain(&self) {
    // Acquire all permits = all workers idle
    let workers = self.semaphore.available_permits() + 1; // at least 1
    let _ = tokio::time::timeout(
      Duration::from_secs(self.build_timeout.as_secs() + 60),
      async {
        for _ in 0..workers {
          if let Ok(permit) = self.semaphore.acquire().await {
            permit.forget(); // don't release back
          }
        }
      },
    )
    .await;
  }

  #[tracing::instrument(skip(self, build), fields(build_id = %build.id, job = %build.job_name))]
  pub fn dispatch(&self, build: Build) {
    if self.drain_token.is_cancelled() {
      tracing::info!(build_id = %build.id, "Drain in progress, not dispatching");
      return;
    }

    let semaphore = self.semaphore.clone();
    let pool = self.pool.clone();
    let work_dir = self.work_dir.clone();
    let timeout = self.build_timeout;
    let log_config = self.log_config.clone();
    let gc_config = self.gc_config.clone();
    let notifications_config = self.notifications_config.clone();
    let signing_config = self.signing_config.clone();
    let cache_upload_config = self.cache_upload_config.clone();

    tokio::spawn(async move {
      let _permit = match semaphore.acquire().await {
        Ok(p) => p,
        Err(_) => return,
      };

      if let Err(e) = run_build(
        &pool,
        &build,
        &work_dir,
        timeout,
        &log_config,
        &gc_config,
        &notifications_config,
        &signing_config,
        &cache_upload_config,
      )
      .await
      {
        tracing::error!(build_id = %build.id, "Build dispatch failed: {e}");
      }
    });
  }
}

/// Query nix path-info for narHash and narSize of an output path.
async fn get_path_info(output_path: &str) -> Option<(String, i64)> {
  let output = tokio::process::Command::new("nix")
    .args(["path-info", "--json", output_path])
    .output()
    .await
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let parsed: serde_json::Value = serde_json::from_str(&stdout).ok()?;

  let entry = parsed.as_array()?.first()?;
  let nar_hash = entry.get("narHash")?.as_str()?.to_string();
  let nar_size = entry.get("narSize")?.as_i64()?;

  Some((nar_hash, nar_size))
}

/// Look up the project that owns a build (build -> evaluation -> jobset ->
/// project).
async fn get_project_for_build(
  pool: &PgPool,
  build: &Build,
) -> Option<(fc_common::models::Project, String)> {
  let eval = repo::evaluations::get(pool, build.evaluation_id)
    .await
    .ok()?;
  let jobset = repo::jobsets::get(pool, eval.jobset_id).await.ok()?;
  let project = repo::projects::get(pool, jobset.project_id).await.ok()?;
  Some((project, eval.commit_hash))
}

/// Sign nix store outputs using the configured signing key.
async fn sign_outputs(
  output_paths: &[String],
  signing_config: &SigningConfig,
) -> bool {
  let key_file = match &signing_config.key_file {
    Some(kf) if signing_config.enabled && kf.exists() => kf,
    _ => return false,
  };

  for output_path in output_paths {
    let result = tokio::process::Command::new("nix")
      .args([
        "store",
        "sign",
        "--key-file",
        &key_file.to_string_lossy(),
        output_path,
      ])
      .output()
      .await;

    match result {
      Ok(o) if o.status.success() => {
        tracing::debug!(output = output_path, "Signed store path");
      },
      Ok(o) => {
        let stderr = String::from_utf8_lossy(&o.stderr);
        tracing::warn!(output = output_path, "Failed to sign: {stderr}");
      },
      Err(e) => {
        tracing::warn!(
          output = output_path,
          "Failed to run nix store sign: {e}"
        );
      },
    }
  }
  true
}

/// Push output paths to an external binary cache via `nix copy`.
async fn push_to_cache(output_paths: &[String], store_uri: &str) {
  for path in output_paths {
    let result = tokio::process::Command::new("nix")
      .args(["copy", "--to", store_uri, path])
      .output()
      .await;
    match result {
      Ok(o) if o.status.success() => {
        tracing::debug!(
          output = path,
          store = store_uri,
          "Pushed to binary cache"
        );
      },
      Ok(o) => {
        let stderr = String::from_utf8_lossy(&o.stderr);
        tracing::warn!(output = path, "Failed to push to cache: {stderr}");
      },
      Err(e) => {
        tracing::warn!(output = path, "Failed to run nix copy: {e}");
      },
    }
  }
}

/// Try to run the build on a remote builder if one is available for the build's
/// system.
async fn try_remote_build(
  pool: &PgPool,
  build: &Build,
  work_dir: &std::path::Path,
  timeout: Duration,
  live_log_path: Option<&std::path::Path>,
) -> Option<crate::builder::BuildResult> {
  let system = build.system.as_deref()?;

  let builders = repo::remote_builders::find_for_system(pool, system)
    .await
    .ok()?;

  for builder in &builders {
    tracing::info!(
        build_id = %build.id,
        builder = %builder.name,
        "Attempting remote build on {}",
        builder.ssh_uri,
    );

    // Set builder_id
    let _ = repo::builds::set_builder(pool, build.id, builder.id).await;

    // Build remotely via --store
    let store_uri = format!("ssh://{}", builder.ssh_uri);
    let result = crate::builder::run_nix_build_remote(
      &build.drv_path,
      work_dir,
      timeout,
      &store_uri,
      builder.ssh_key_file.as_deref(),
      live_log_path,
    )
    .await;

    match result {
      Ok(r) => return Some(r),
      Err(e) => {
        tracing::warn!(
            build_id = %build.id,
            builder = %builder.name,
            "Remote build failed: {e}, trying next builder"
        );
      },
    }
  }

  None
}

#[tracing::instrument(skip(pool, build, work_dir, log_config, gc_config, notifications_config, signing_config, cache_upload_config), fields(build_id = %build.id, job = %build.job_name))]
async fn run_build(
  pool: &PgPool,
  build: &Build,
  work_dir: &std::path::Path,
  timeout: Duration,
  log_config: &LogConfig,
  gc_config: &GcConfig,
  notifications_config: &NotificationsConfig,
  signing_config: &SigningConfig,
  cache_upload_config: &CacheUploadConfig,
) -> anyhow::Result<()> {
  // Atomically claim the build
  let claimed = repo::builds::start(pool, build.id).await?;
  if claimed.is_none() {
    tracing::debug!(build_id = %build.id, "Build already claimed, skipping");
    return Ok(());
  }

  tracing::info!(build_id = %build.id, job = %build.job_name, "Starting build");

  // Create a build step record
  let step = repo::build_steps::create(pool, CreateBuildStep {
    build_id:    build.id,
    step_number: 1,
    command:     format!(
      "nix build --no-link --print-out-paths {}",
      build.drv_path
    ),
  })
  .await?;

  // Set up live log path
  let live_log_path =
    log_config.log_dir.join(format!("{}.active.log", build.id));
  let _ = tokio::fs::create_dir_all(&log_config.log_dir).await;

  // Try remote build first, then fall back to local
  let result = if build.system.is_some() {
    match try_remote_build(pool, build, work_dir, timeout, Some(&live_log_path))
      .await
    {
      Some(r) => Ok(r),
      None => {
        // No remote builder available or all failed — build locally
        crate::builder::run_nix_build(
          &build.drv_path,
          work_dir,
          timeout,
          Some(&live_log_path),
        )
        .await
      },
    }
  } else {
    crate::builder::run_nix_build(
      &build.drv_path,
      work_dir,
      timeout,
      Some(&live_log_path),
    )
    .await
  };

  // Initialize log storage
  let log_storage = LogStorage::new(log_config.log_dir.clone()).ok();

  match result {
    Ok(build_result) => {
      // Complete the build step
      let exit_code = if build_result.success { 0 } else { 1 };
      repo::build_steps::complete(
        pool,
        step.id,
        exit_code,
        Some(&build_result.stdout),
        Some(&build_result.stderr),
      )
      .await?;

      // Create sub-step records from parsed nix log
      for (i, sub_step) in build_result.sub_steps.iter().enumerate() {
        let sub = repo::build_steps::create(pool, CreateBuildStep {
          build_id:    build.id,
          step_number: (i as i32) + 2,
          command:     format!("nix build {}", sub_step.drv_path),
        })
        .await?;
        let sub_exit = if sub_step.success { 0 } else { 1 };
        repo::build_steps::complete(pool, sub.id, sub_exit, None, None).await?;
      }

      // Write build log (rename active log to final)
      let log_path = if let Some(ref storage) = log_storage {
        let final_path = storage.log_path(&build.id);
        if live_log_path.exists() {
          let _ = tokio::fs::rename(&live_log_path, &final_path).await;
        } else {
          match storage.write_log(
            &build.id,
            &build_result.stdout,
            &build_result.stderr,
          ) {
            Ok(_) => {},
            Err(e) => {
              tracing::warn!(build_id = %build.id, "Failed to write build log: {e}");
            },
          }
        }
        Some(final_path.to_string_lossy().to_string())
      } else {
        None
      };

      if build_result.success {
        // Parse output names from build's outputs JSON
        let output_names: Vec<String> = build
          .outputs
          .as_ref()
          .and_then(|v| v.as_object())
          .map(|obj| obj.keys().cloned().collect())
          .unwrap_or_default();

        // Register GC roots and create build products for each output
        for (i, output_path) in build_result.output_paths.iter().enumerate() {
          let output_name = output_names.get(i).cloned().unwrap_or_else(|| {
            if i == 0 {
              build.job_name.clone()
            } else {
              format!("{}-{i}", build.job_name)
            }
          });

          // Register GC root
          let mut gc_root_path = None;
          if let Ok(gc_roots) =
            GcRoots::new(gc_config.gc_roots_dir.clone(), gc_config.enabled)
          {
            let gc_id = if i == 0 {
              build.id
            } else {
              uuid::Uuid::new_v4()
            };
            match gc_roots.register(&gc_id, output_path) {
              Ok(Some(link_path)) => {
                gc_root_path = Some(link_path.to_string_lossy().to_string());
              },
              Ok(None) => {},
              Err(e) => {
                tracing::warn!(build_id = %build.id, "Failed to register GC root: {e}");
              },
            }
          }

          // Get metadata from nix path-info
          let (sha256_hash, file_size) = match get_path_info(output_path).await
          {
            Some((hash, size)) => (Some(hash), Some(size)),
            None => (None, None),
          };

          let product =
            repo::build_products::create(pool, CreateBuildProduct {
              build_id: build.id,
              name: output_name,
              path: output_path.clone(),
              sha256_hash,
              file_size,
              content_type: None,
              is_directory: true,
            })
            .await?;

          // Update the build product with GC root path if registered
          if gc_root_path.is_some() {
            sqlx::query(
              "UPDATE build_products SET gc_root_path = $1 WHERE id = $2",
            )
            .bind(&gc_root_path)
            .bind(product.id)
            .execute(pool)
            .await?;
          }
        }

        // Sign outputs at build time
        if sign_outputs(&build_result.output_paths, signing_config).await {
          let _ = repo::builds::mark_signed(pool, build.id).await;
        }

        // Push to external binary cache if configured
        if cache_upload_config.enabled
          && let Some(ref store_uri) = cache_upload_config.store_uri
        {
          push_to_cache(&build_result.output_paths, store_uri).await;
        }

        let primary_output =
          build_result.output_paths.first().map(|s| s.as_str());

        repo::builds::complete(
          pool,
          build.id,
          BuildStatus::Completed,
          log_path.as_deref(),
          primary_output,
          None,
        )
        .await?;

        tracing::info!(build_id = %build.id, "Build completed successfully");
      } else {
        // Check if we should retry
        if build.retry_count < build.max_retries {
          tracing::info!(
              build_id = %build.id,
              retry = build.retry_count + 1,
              max = build.max_retries,
              "Build failed, scheduling retry"
          );
          sqlx::query(
            "UPDATE builds SET status = 'pending', started_at = NULL, \
             retry_count = retry_count + 1, completed_at = NULL WHERE id = $1",
          )
          .bind(build.id)
          .execute(pool)
          .await?;
          // Clean up live log
          let _ = tokio::fs::remove_file(&live_log_path).await;
          return Ok(());
        }

        repo::builds::complete(
          pool,
          build.id,
          BuildStatus::Failed,
          log_path.as_deref(),
          None,
          Some(&build_result.stderr),
        )
        .await?;

        tracing::warn!(build_id = %build.id, "Build failed");
      }
    },
    Err(e) => {
      let msg = e.to_string();

      // Write error log
      if let Some(ref storage) = log_storage {
        let _ = storage.write_log(&build.id, "", &msg);
      }
      // Clean up live log
      let _ = tokio::fs::remove_file(&live_log_path).await;

      repo::build_steps::complete(pool, step.id, 1, None, Some(&msg)).await?;
      repo::builds::complete(
        pool,
        build.id,
        BuildStatus::Failed,
        None,
        None,
        Some(&msg),
      )
      .await?;
      tracing::error!(build_id = %build.id, "Build error: {msg}");
    },
  }

  // Dispatch notifications after build completion
  let updated_build = repo::builds::get(pool, build.id).await?;
  if updated_build.status == BuildStatus::Completed
    || updated_build.status == BuildStatus::Failed
  {
    if let Some((project, commit_hash)) =
      get_project_for_build(pool, build).await
    {
      fc_common::notifications::dispatch_build_finished(
        &updated_build,
        &project,
        &commit_hash,
        notifications_config,
      )
      .await;
    }

    // Auto-promote channels if all builds in the evaluation are done
    if updated_build.status == BuildStatus::Completed
      && let Ok(eval) = repo::evaluations::get(pool, build.evaluation_id).await
    {
      let _ =
        repo::channels::auto_promote_if_complete(pool, eval.jobset_id, eval.id)
          .await;
    }
  }

  Ok(())
}

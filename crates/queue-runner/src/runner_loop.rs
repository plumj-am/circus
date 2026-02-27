use std::{sync::Arc, time::Duration};

use fc_common::{
  models::{BuildStatus, JobsetState},
  repo,
};
use sqlx::PgPool;
use tokio::sync::Notify;

use crate::worker::WorkerPool;

/// Main queue runner loop. Polls for pending builds and dispatches them to
/// workers.
///
/// # Errors
///
/// Returns error if database operations fail and `strict_errors` is enabled.
pub async fn run(
  pool: PgPool,
  worker_pool: Arc<WorkerPool>,
  poll_interval: Duration,
  wakeup: Arc<Notify>,
  strict_errors: bool,
  failed_paths_cache: bool,
) -> anyhow::Result<()> {
  // Reset orphaned builds from previous crashes (older than 5 minutes)
  match repo::builds::reset_orphaned(&pool, 300).await {
    Ok(count) if count > 0 => {
      tracing::warn!(count, "Reset orphaned builds back to pending");
    },
    Ok(_) => {},
    Err(e) => {
      tracing::error!("Failed to reset orphaned builds: {e}");
    },
  }

  loop {
    let wc = worker_pool.worker_count() as i32;
    match repo::builds::list_pending(&pool, 10, wc).await {
      Ok(builds) => {
        if !builds.is_empty() {
          tracing::info!("Found {} pending builds", builds.len());
        }
        for build in builds {
          // Aggregate builds: check if all constituents are done
          if build.is_aggregate {
            match repo::build_dependencies::all_deps_completed(&pool, build.id)
              .await
            {
              Ok(true) => {
                // All constituents done, mark aggregate as completed
                tracing::info!(
                    build_id = %build.id,
                    job = %build.job_name,
                    "Aggregate build: all constituents completed"
                );
                if let Err(e) = repo::builds::start(&pool, build.id).await {
                  tracing::warn!(build_id = %build.id, "Failed to start aggregate build: {e}");
                }
                if let Err(e) = repo::builds::complete(
                  &pool,
                  build.id,
                  BuildStatus::Succeeded,
                  None,
                  None,
                  None,
                )
                .await
                {
                  tracing::warn!(build_id = %build.id, "Failed to complete aggregate build: {e}");
                }
                continue;
              },
              Ok(false) => {
                tracing::debug!(
                    build_id = %build.id,
                    "Aggregate build waiting for constituents"
                );
                continue;
              },
              Err(e) => {
                tracing::error!(
                    build_id = %build.id,
                    "Failed to check aggregate deps: {e}"
                );
                continue;
              },
            }
          }

          // Derivation deduplication: reuse result if same drv was already
          // built
          match repo::builds::get_completed_by_drv_path(&pool, &build.drv_path)
            .await
          {
            Ok(Some(existing)) if existing.id != build.id => {
              tracing::info!(
                  build_id = %build.id,
                  existing_id = %existing.id,
                  drv = %build.drv_path,
                  "Dedup: reusing result from existing build"
              );
              if let Err(e) = repo::builds::start(&pool, build.id).await {
                tracing::warn!(build_id = %build.id, "Failed to start dedup build: {e}");
              }
              if let Err(e) = repo::builds::complete(
                &pool,
                build.id,
                BuildStatus::Succeeded,
                existing.log_path.as_deref(),
                existing.build_output_path.as_deref(),
                None,
              )
              .await
              {
                tracing::warn!(build_id = %build.id, "Failed to complete dedup build: {e}");
              }
              continue;
            },
            _ => {},
          }

          // Failed paths cache: skip known-failing derivations
          if failed_paths_cache
            && matches!(
              repo::failed_paths_cache::is_cached_failure(
                &pool,
                &build.drv_path,
              )
              .await,
              Ok(true)
            )
          {
            tracing::info!(
                build_id = %build.id, drv = %build.drv_path,
                "Cached failure: skipping known-failing derivation"
            );
            if let Err(e) = repo::builds::start(&pool, build.id).await {
              tracing::warn!(build_id = %build.id, "Failed to start cached-failure build: {e}");
            }
            if let Err(e) = repo::builds::complete(
              &pool,
              build.id,
              BuildStatus::CachedFailure,
              None,
              None,
              Some("Build skipped: derivation is in failed paths cache"),
            )
            .await
            {
              tracing::warn!(build_id = %build.id, "Failed to complete cached-failure build: {e}");
            }
            continue;
          }

          // Dependency-aware scheduling: skip if deps not met
          match repo::build_dependencies::all_deps_completed(&pool, build.id)
            .await
          {
            Ok(true) => {},
            Ok(false) => {
              tracing::debug!(
                  build_id = %build.id,
                  "Build waiting for dependencies"
              );
              continue;
            },
            Err(e) => {
              tracing::error!(
                  build_id = %build.id,
                  "Failed to check build deps: {e}"
              );
              continue;
            },
          }

          // One-at-a-time scheduling: check if jobset allows concurrent builds
          // First, get the evaluation to find the jobset
          let eval =
            match repo::evaluations::get(&pool, build.evaluation_id).await {
              Ok(eval) => eval,
              Err(e) => {
                tracing::error!(
                    build_id = %build.id,
                    evaluation_id = %build.evaluation_id,
                    "Failed to get evaluation for one-at-a-time check: {e}"
                );
                continue;
              },
            };

          let jobset = match repo::jobsets::get(&pool, eval.jobset_id).await {
            Ok(jobset) => jobset,
            Err(e) => {
              tracing::error!(
                  build_id = %build.id,
                  jobset_id = %eval.jobset_id,
                  "Failed to get jobset for one-at-a-time check: {e}"
              );
              continue;
            },
          };

          if jobset.state == JobsetState::OneAtATime {
            match repo::jobsets::has_running_builds(&pool, jobset.id).await {
              Ok(true) => {
                tracing::debug!(
                    build_id = %build.id,
                    jobset = %jobset.name,
                    "One-at-a-time: skipping, another build is running"
                );
                continue;
              },
              Ok(false) => {},
              Err(e) => {
                tracing::error!(
                    build_id = %build.id,
                    "Failed to check running builds: {e}"
                );
                continue;
              },
            }
          }

          worker_pool.dispatch(build);
        }
      },
      Err(e) => {
        if strict_errors {
          return Err(anyhow::anyhow!("Failed to fetch pending builds: {e}"));
        }
        tracing::error!("Failed to fetch pending builds: {e}");
      },
    }
    // Wake on NOTIFY or fall back to regular poll interval
    let _ = tokio::time::timeout(poll_interval, wakeup.notified()).await;
  }
}

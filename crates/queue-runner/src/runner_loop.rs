use std::{sync::Arc, time::Duration};

use fc_common::{models::BuildStatus, repo};
use sqlx::PgPool;

use crate::worker::WorkerPool;

pub async fn run(
  pool: PgPool,
  worker_pool: Arc<WorkerPool>,
  poll_interval: Duration,
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
    match repo::builds::list_pending(&pool, 10).await {
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
                // All constituents done — mark aggregate as completed
                tracing::info!(
                    build_id = %build.id,
                    job = %build.job_name,
                    "Aggregate build: all constituents completed"
                );
                let _ = repo::builds::start(&pool, build.id).await;
                let _ = repo::builds::complete(
                  &pool,
                  build.id,
                  BuildStatus::Completed,
                  None,
                  None,
                  None,
                )
                .await;
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
              let _ = repo::builds::start(&pool, build.id).await;
              let _ = repo::builds::complete(
                &pool,
                build.id,
                BuildStatus::Completed,
                existing.log_path.as_deref(),
                existing.build_output_path.as_deref(),
                None,
              )
              .await;
              continue;
            },
            _ => {},
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

          worker_pool.dispatch(build);
        }
      },
      Err(e) => {
        tracing::error!("Failed to fetch pending builds: {e}");
      },
    }
    tokio::time::sleep(poll_interval).await;
  }
}

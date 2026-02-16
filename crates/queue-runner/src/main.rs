use std::{sync::Arc, time::Duration};

use clap::Parser;
use fc_common::{
  config::{Config, GcConfig},
  database::Database,
  gc_roots,
};
use fc_queue_runner::worker::WorkerPool;

#[derive(Parser)]
#[command(name = "fc-queue-runner")]
#[command(about = "CI Queue Runner - Build dispatch and execution")]
struct Cli {
  #[arg(short, long)]
  workers: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let cli = Cli::parse();

  let config = Config::load()?;
  fc_common::init_tracing(&config.tracing);

  tracing::info!("Starting CI Queue Runner");
  let log_config = config.logs;
  let gc_config = config.gc;
  let gc_config_for_loop = gc_config.clone();
  let notifications_config = config.notifications;
  let signing_config = config.signing;
  let cache_upload_config = config.cache_upload;
  let qr_config = config.queue_runner;

  let workers = cli.workers.unwrap_or(qr_config.workers);
  let poll_interval = Duration::from_secs(qr_config.poll_interval);
  let build_timeout = Duration::from_secs(qr_config.build_timeout);
  let strict_errors = qr_config.strict_errors;
  let failed_paths_cache = qr_config.failed_paths_cache;
  let failed_paths_ttl = qr_config.failed_paths_ttl;
  let work_dir = qr_config.work_dir;

  // Ensure the work directory exists
  tokio::fs::create_dir_all(&work_dir).await?;

  // Clean up orphaned active logs from previous crashes
  cleanup_stale_logs(&log_config.log_dir).await;

  let db = Database::new(config.database).await?;

  let worker_pool = Arc::new(WorkerPool::new(
    db.pool().clone(),
    workers,
    work_dir.clone(),
    build_timeout,
    log_config,
    gc_config,
    notifications_config.clone(),
    signing_config,
    cache_upload_config,
    notifications_config.alerts.clone(),
  ));

  tracing::info!(
      workers = workers,
      poll_interval = ?poll_interval,
      build_timeout = ?build_timeout,
      work_dir = %work_dir.display(),
      "Queue runner configured"
  );

  let worker_pool_for_drain = worker_pool.clone();

  let wakeup = Arc::new(tokio::sync::Notify::new());
  let listener_handle = fc_common::pg_notify::spawn_listener(
    db.pool(),
    &[fc_common::pg_notify::CHANNEL_BUILDS_CHANGED],
    wakeup.clone(),
  );

  tokio::select! {
      result = fc_queue_runner::runner_loop::run(db.pool().clone(), worker_pool, poll_interval, wakeup, strict_errors, failed_paths_cache) => {
          if let Err(e) = result {
              tracing::error!("Runner loop failed: {e}");
          }
      }
      () = gc_loop(gc_config_for_loop) => {}
      () = failed_paths_cleanup_loop(db.pool().clone(), failed_paths_ttl, failed_paths_cache) => {}
      () = shutdown_signal() => {
          tracing::info!("Shutdown signal received, draining in-flight builds...");
          worker_pool_for_drain.drain();
          worker_pool_for_drain.wait_for_drain().await;
          tracing::info!("All in-flight builds completed");
      }
  }

  listener_handle.abort();
  let _ = listener_handle.await;

  tracing::info!("Queue runner shutting down, closing database pool");
  db.close().await;

  Ok(())
}

async fn cleanup_stale_logs(log_dir: &std::path::Path) {
  if let Ok(mut entries) = tokio::fs::read_dir(log_dir).await {
    while let Ok(Some(entry)) = entries.next_entry().await {
      if entry.file_name().to_string_lossy().ends_with(".active.log") {
        let _ = tokio::fs::remove_file(entry.path()).await;
        tracing::info!("Removed stale active log: {}", entry.path().display());
      }
    }
  }
}

async fn gc_loop(gc_config: GcConfig) {
  if !gc_config.enabled {
    return std::future::pending().await;
  }
  let interval = std::time::Duration::from_secs(gc_config.cleanup_interval);
  let max_age = std::time::Duration::from_secs(gc_config.max_age_days * 86400);

  loop {
    tokio::time::sleep(interval).await;
    match gc_roots::cleanup_old_roots(&gc_config.gc_roots_dir, max_age) {
      Ok(count) if count > 0 => {
        tracing::info!(count, "Cleaned up old GC roots");
        // Optionally run nix-collect-garbage
        match tokio::process::Command::new("nix-collect-garbage")
          .output()
          .await
        {
          Ok(output) if output.status.success() => {
            tracing::info!("nix-collect-garbage completed");
          },
          Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("nix-collect-garbage failed: {stderr}");
          },
          Err(e) => {
            tracing::warn!("Failed to run nix-collect-garbage: {e}");
          },
        }
      },
      Ok(_) => {},
      Err(e) => {
        tracing::error!("GC cleanup failed: {e}");
      },
    }
  }
}

async fn failed_paths_cleanup_loop(
  pool: sqlx::PgPool,
  ttl: u64,
  enabled: bool,
) {
  if !enabled {
    return std::future::pending().await;
  }

  let interval = std::time::Duration::from_secs(3600);
  loop {
    tokio::time::sleep(interval).await;
    match fc_common::repo::failed_paths_cache::cleanup_expired(&pool, ttl).await
    {
      Ok(count) if count > 0 => {
        tracing::info!(count, "Cleaned up expired failed paths cache entries");
      },
      Ok(_) => {},
      Err(e) => {
        tracing::error!("Failed paths cache cleanup failed: {e}");
      },
    }
  }
}

async fn shutdown_signal() {
  let ctrl_c = async {
    tokio::signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
      .expect("failed to install SIGTERM handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
      () = ctrl_c => {},
      () = terminate => {},
  }
}

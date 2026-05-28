//! Liveness heartbeat for background services.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::error::{CiError, Result};

/// Canonical service identifiers. Use these constants instead of
/// stringly-typed names to avoid divergence between writers and readers.
pub const SERVICE_EVALUATOR: &str = "evaluator";
pub const SERVICE_QUEUE_RUNNER: &str = "queue-runner";

/// A snapshot of one service's reported liveness.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Heartbeat {
  pub service:               String,
  pub last_heartbeat_at:     chrono::DateTime<chrono::Utc>,
  pub poll_interval_seconds: i32,
  pub version:               Option<String>,
  pub extra:                 serde_json::Value,
}

/// Upsert a heartbeat for `service`. Idempotent; safe to call on every poll
/// tick. `poll_interval_seconds` tells the health endpoint what staleness
/// threshold to apply.
///
/// # Errors
///
/// Returns error if the database operation fails.
pub async fn record(
  pool: &PgPool,
  service: &str,
  poll_interval_seconds: u32,
  version: Option<&str>,
) -> Result<()> {
  let poll_i32: i32 = poll_interval_seconds.try_into().map_err(|_| {
    CiError::Validation(format!(
      "poll_interval_seconds {poll_interval_seconds} does not fit in i32"
    ))
  })?;

  sqlx::query(
    "INSERT INTO service_heartbeats (service, last_heartbeat_at, \
     poll_interval_seconds, version) VALUES ($1, NOW(), $2, $3) ON CONFLICT \
     (service) DO UPDATE SET last_heartbeat_at     = \
     EXCLUDED.last_heartbeat_at, poll_interval_seconds = \
     EXCLUDED.poll_interval_seconds, version               = EXCLUDED.version",
  )
  .bind(service)
  .bind(poll_i32)
  .bind(version)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;

  Ok(())
}

/// One service's liveness state as judged by the server.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
  pub service:           String,
  pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
  /// Seconds since the last heartbeat, computed by the database to avoid
  /// client-clock skew. `None` if the service has never reported.
  pub seconds_since:     Option<f64>,
  /// Configured poll interval the service most recently reported.
  pub poll_interval:     Option<i32>,
  /// True when the heartbeat is fresh; false when stale or missing.
  pub healthy:           bool,
  /// Optional human-readable hint about why this service is unhealthy.
  pub detail:            Option<String>,
}

/// Read all heartbeats and compute liveness, using the database's `NOW()`
/// for the comparison so we are immune to local clock skew.
///
/// `threshold_multiplier` is how many `poll_interval_seconds` we tolerate
/// before declaring a service stale. `3` is a reasonable default: it allows
/// for one missed tick plus jitter without firing false alarms.
///
/// `expected_services` is the set of services that MUST exist; any expected
/// service that has never reported a heartbeat is included in the output as
/// `unhealthy` with a "never reported" detail.
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn status_for(
  pool: &PgPool,
  expected_services: &[&str],
  threshold_multiplier: u32,
) -> Result<Vec<ServiceStatus>> {
  let rows: Vec<(String, chrono::DateTime<chrono::Utc>, f64, i32)> =
    sqlx::query_as(
      "SELECT service, last_heartbeat_at, EXTRACT(EPOCH FROM (NOW() - \
       last_heartbeat_at))::float8 AS seconds_since, poll_interval_seconds \
       FROM service_heartbeats",
    )
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)?;

  let mut out: Vec<ServiceStatus> = Vec::new();
  let mut reported = std::collections::HashSet::new();

  for (service, last_heartbeat_at, seconds_since, poll_interval) in rows {
    reported.insert(service.clone());

    let limit = f64::from(poll_interval) * f64::from(threshold_multiplier);
    let healthy = seconds_since <= limit;
    let detail = if healthy {
      None
    } else {
      Some(format!(
        "stale heartbeat: {seconds_since:.1}s since last report (limit \
         {limit:.1}s = {poll_interval}s poll x {threshold_multiplier})"
      ))
    };

    out.push(ServiceStatus {
      service,
      last_heartbeat_at: Some(last_heartbeat_at),
      seconds_since: Some(seconds_since),
      poll_interval: Some(poll_interval),
      healthy,
      detail,
    });
  }

  for expected in expected_services {
    if !reported.contains(*expected) {
      out.push(ServiceStatus {
        service:           (*expected).to_string(),
        last_heartbeat_at: None,
        seconds_since:     None,
        poll_interval:     None,
        healthy:           false,
        detail:            Some(
          "service has never reported a heartbeat".to_string(),
        ),
      });
    }
  }

  out.sort_by(|a, b| a.service.cmp(&b.service));
  Ok(out)
}

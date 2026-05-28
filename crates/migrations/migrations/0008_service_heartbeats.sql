-- Service heartbeat table.
--
-- The server runs in a separate process from the evaluator and queue-runner,
-- so it cannot observe their liveness directly. Each background service
-- writes its heartbeat row on every poll; the /health endpoint reads this
-- table and reports a service as degraded when its heartbeat is older than
-- a small multiple of its poll interval.
--
-- Identity is the service name (PRIMARY KEY) so writes are upserts; there is
-- always at most one row per service. The `extra` jsonb column lets services
-- attach lightweight status hints (queue depth, currently-running evals, etc.)
-- without further schema changes.
CREATE TABLE IF NOT EXISTS service_heartbeats (
  service TEXT PRIMARY KEY,
  last_heartbeat_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  poll_interval_seconds INTEGER NOT NULL CHECK (poll_interval_seconds > 0),
  version TEXT,
  extra JSONB NOT NULL DEFAULT '{}'::jsonb
);

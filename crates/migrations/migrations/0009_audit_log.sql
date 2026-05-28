-- Audit log for security-relevant mutating actions.
--
-- Every admin action (builder mutation, config write, project deletion),
-- every credential mutation (user CRUD, api-key CRUD, password change),
-- and every authentication outcome (login success/failure, logout) writes
-- a row here. The table is append-only from the application's perspective;
-- it grows unbounded by design so retention is an operator decision (drop
-- rows older than N days via a manual job or partition rotation).
--
-- Identity is captured with both an actor_id (FK-loose, since the
-- referenced row may be deleted later) and a denormalized actor_name so
-- the log remains readable after the referenced user/key is gone.

CREATE TABLE IF NOT EXISTS audit_log (
  id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
  occurred_at  TIMESTAMPTZ NOT NULL    DEFAULT NOW(),

  -- Who: 'api_key', 'user', or 'anonymous'.
  actor_kind   TEXT        NOT NULL,
  actor_id     UUID,
  actor_name   TEXT,

  -- What: stable, uppercase action code (e.g. LOGIN_SUCCESS, BUILDER_DELETE).
  action       TEXT        NOT NULL,

  -- On what: object class (e.g. 'builder', 'api_key', 'user', 'config') and
  -- its identifier as a string (uuid or other natural key).
  target_kind  TEXT,
  target_id    TEXT,

  -- Free-form structured detail (old value, new value, request metadata).
  details      JSONB       NOT NULL    DEFAULT '{}'::jsonb,

  -- Network identity of the actor at the time of action (caller IP).
  remote_addr  TEXT,

  CHECK (actor_kind IN ('api_key', 'user', 'anonymous'))
);

CREATE INDEX IF NOT EXISTS idx_audit_log_occurred_at
  ON audit_log (occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_actor_id
  ON audit_log (actor_id)
  WHERE actor_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_audit_log_action
  ON audit_log (action);

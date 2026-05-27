-- circus database schema - trigger functions and triggers

-- Trigger function: auto-update updated_at on mutation
CREATE OR REPLACE FUNCTION update_updated_at_column () RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_projects_updated_at BEFORE
UPDATE ON projects FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column ();

CREATE TRIGGER update_jobsets_updated_at BEFORE
UPDATE ON jobsets FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column ();

CREATE TRIGGER update_users_updated_at BEFORE
UPDATE ON users FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column ();

-- Trigger functions: LISTEN/NOTIFY for event-driven daemon wakeup
CREATE OR REPLACE FUNCTION notify_builds_changed () RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('circus_builds_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION notify_jobsets_changed () RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('circus_jobsets_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_builds_insert_notify
AFTER INSERT ON builds FOR EACH ROW
EXECUTE FUNCTION notify_builds_changed ();

CREATE TRIGGER trg_builds_status_notify
AFTER
UPDATE ON builds FOR EACH ROW WHEN (OLD.status IS DISTINCT FROM NEW.status)
EXECUTE FUNCTION notify_builds_changed ();

CREATE TRIGGER trg_jobsets_insert_notify
AFTER INSERT ON jobsets FOR EACH ROW
EXECUTE FUNCTION notify_jobsets_changed ();

CREATE TRIGGER trg_jobsets_update_notify
AFTER
UPDATE ON jobsets FOR EACH ROW WHEN (
  OLD.enabled IS DISTINCT FROM NEW.enabled
  OR OLD.state IS DISTINCT FROM NEW.state
  OR OLD.nix_expression IS DISTINCT FROM NEW.nix_expression
  OR OLD.check_interval IS DISTINCT FROM NEW.check_interval
)
EXECUTE FUNCTION notify_jobsets_changed ();

CREATE TRIGGER trg_jobsets_delete_notify
AFTER DELETE ON jobsets FOR EACH ROW
EXECUTE FUNCTION notify_jobsets_changed ();

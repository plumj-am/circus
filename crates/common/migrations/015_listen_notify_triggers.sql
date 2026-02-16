-- PostgreSQL LISTEN/NOTIFY triggers for event-driven reactivity
-- Emits notifications on builds/jobsets mutations so daemons can wake immediately

-- Trigger function: notify on builds changes
CREATE OR REPLACE FUNCTION notify_builds_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('fc_builds_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Trigger function: notify on jobsets changes
CREATE OR REPLACE FUNCTION notify_jobsets_changed() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('fc_jobsets_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

-- Builds: new build inserted (queue-runner should wake)
CREATE TRIGGER trg_builds_insert_notify
    AFTER INSERT ON builds
    FOR EACH ROW
    EXECUTE FUNCTION notify_builds_changed();

-- Builds: status changed (queue-runner should re-check, e.g. deps resolved)
CREATE TRIGGER trg_builds_status_notify
    AFTER UPDATE ON builds
    FOR EACH ROW
    WHEN (OLD.status IS DISTINCT FROM NEW.status)
    EXECUTE FUNCTION notify_builds_changed();

-- Jobsets: new jobset created (evaluator should wake)
CREATE TRIGGER trg_jobsets_insert_notify
    AFTER INSERT ON jobsets
    FOR EACH ROW
    EXECUTE FUNCTION notify_jobsets_changed();

-- Jobsets: relevant fields changed (evaluator should re-check)
CREATE TRIGGER trg_jobsets_update_notify
    AFTER UPDATE ON jobsets
    FOR EACH ROW
    WHEN (
        OLD.enabled IS DISTINCT FROM NEW.enabled
        OR OLD.state IS DISTINCT FROM NEW.state
        OR OLD.nix_expression IS DISTINCT FROM NEW.nix_expression
        OR OLD.check_interval IS DISTINCT FROM NEW.check_interval
    )
    EXECUTE FUNCTION notify_jobsets_changed();

-- Jobsets: deleted (evaluator should wake to stop tracking)
CREATE TRIGGER trg_jobsets_delete_notify
    AFTER DELETE ON jobsets
    FOR EACH ROW
    EXECUTE FUNCTION notify_jobsets_changed();

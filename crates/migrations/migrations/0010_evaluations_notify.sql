-- Wake the evaluator on push-driven evaluations.
--
-- Webhooks (GitHub/Gitea/Forgejo/GitLab push and PR events) and the
-- POST /evaluations/trigger API insert pending evaluation rows directly
-- rather than mutating jobsets. Without this trigger the evaluator only
-- sees that work on its next poll_interval tick (worst case: tens of
-- seconds to a minute), and even then its check_interval filter can hide
-- a jobset whose last_checked_at is fresh. Push the wake-up via NOTIFY
-- so the daemon picks the work up within milliseconds.
CREATE OR REPLACE FUNCTION notify_evaluations_changed () RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('circus_jobsets_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME,
        'jobset_id', NEW.jobset_id
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_evaluations_insert_notify
AFTER INSERT ON evaluations FOR EACH ROW
EXECUTE FUNCTION notify_evaluations_changed ();

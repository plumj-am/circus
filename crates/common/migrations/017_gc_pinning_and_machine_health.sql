-- GC pinning (#11)
ALTER TABLE builds ADD COLUMN IF NOT EXISTS keep BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE jobsets ADD COLUMN IF NOT EXISTS keep_nr INTEGER NOT NULL DEFAULT 3;

-- Recreate active_jobsets view to include keep_nr
DROP VIEW IF EXISTS active_jobsets;
CREATE VIEW active_jobsets AS
SELECT
    j.id,
    j.project_id,
    j.name,
    j.nix_expression,
    j.enabled,
    j.flake_mode,
    j.check_interval,
    j.branch,
    j.scheduling_shares,
    j.created_at,
    j.updated_at,
    j.state,
    j.last_checked_at,
    j.keep_nr,
    p.name as project_name,
    p.repository_url
FROM jobsets j
JOIN projects p ON j.project_id = p.id
WHERE j.state IN ('enabled', 'one_shot', 'one_at_a_time');

-- Machine health tracking (#5)
ALTER TABLE remote_builders ADD COLUMN IF NOT EXISTS consecutive_failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE remote_builders ADD COLUMN IF NOT EXISTS disabled_until TIMESTAMP WITH TIME ZONE;
ALTER TABLE remote_builders ADD COLUMN IF NOT EXISTS last_failure TIMESTAMP WITH TIME ZONE;

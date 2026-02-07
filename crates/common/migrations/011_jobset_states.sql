-- Migration: Add jobset states for Hydra-compatible scheduling
-- Supports 4 states: disabled, enabled, one_shot, one_at_a_time

-- Add state column with CHECK constraint
ALTER TABLE jobsets ADD COLUMN state VARCHAR(50) NOT NULL DEFAULT 'enabled'
    CHECK (state IN ('disabled', 'enabled', 'one_shot', 'one_at_a_time'));

-- Migrate existing data based on enabled column
UPDATE jobsets SET state = CASE WHEN enabled THEN 'enabled' ELSE 'disabled' END;

-- Add last_checked_at for per-jobset interval tracking
ALTER TABLE jobsets ADD COLUMN last_checked_at TIMESTAMP WITH TIME ZONE;

-- Drop and recreate active_jobsets view to include new columns
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
    p.name as project_name,
    p.repository_url
FROM jobsets j
JOIN projects p ON j.project_id = p.id
WHERE j.state IN ('enabled', 'one_shot', 'one_at_a_time');

-- Indexes for efficient queries
CREATE INDEX idx_jobsets_state ON jobsets(state);
CREATE INDEX idx_jobsets_last_checked_at ON jobsets(last_checked_at);

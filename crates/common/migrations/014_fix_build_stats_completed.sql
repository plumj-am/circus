-- Fix build_stats view and data after 'completed' -> 'succeeded' status rename

-- Migrate any existing builds still using the old status value
UPDATE builds SET status = 'succeeded' WHERE status = 'completed';

-- Recreate the build_stats view to reference the new status
DROP VIEW IF EXISTS build_stats;
CREATE VIEW build_stats AS
SELECT
    COUNT(*) as total_builds,
    COUNT(CASE WHEN status = 'succeeded' THEN 1 END) as completed_builds,
    COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_builds,
    COUNT(CASE WHEN status = 'running' THEN 1 END) as running_builds,
    COUNT(CASE WHEN status = 'pending' THEN 1 END) as pending_builds,
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_duration_seconds
FROM builds
WHERE started_at IS NOT NULL;

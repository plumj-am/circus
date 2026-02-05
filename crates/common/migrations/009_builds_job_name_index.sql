-- Add index on builds.job_name for ILIKE queries in list_filtered
CREATE INDEX IF NOT EXISTS idx_builds_job_name ON builds (job_name);

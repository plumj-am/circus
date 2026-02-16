-- Extended build status codes to match Hydra

-- Update the builds table CHECK constraint to include all new statuses
ALTER TABLE builds DROP CONSTRAINT builds_status_check;

ALTER TABLE builds ADD CONSTRAINT builds_status_check CHECK (
    status IN (
        'pending',
        'running',
        'succeeded',
        'failed',
        'dependency_failed',
        'aborted',
        'cancelled',
        'failed_with_output',
        'timeout',
        'cached_failure',
        'unsupported_system',
        'log_limit_exceeded',
        'nar_size_limit_exceeded',
        'non_deterministic'
    )
);

-- Add index on status for faster filtering
CREATE INDEX IF NOT EXISTS idx_builds_status ON builds(status);

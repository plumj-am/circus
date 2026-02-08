-- Migration: Add build metrics collection
-- Stores timing, size, and performance metrics for builds

-- Create build_metrics table
CREATE TABLE build_metrics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    build_id UUID NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    metric_name VARCHAR(100) NOT NULL,
    metric_value DOUBLE PRECISION NOT NULL,
    unit VARCHAR(50) NOT NULL,
    collected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Index for efficient lookups by build
CREATE INDEX idx_build_metrics_build_id ON build_metrics(build_id);

-- Index for time-based queries (alerting)
CREATE INDEX idx_build_metrics_collected_at ON build_metrics(collected_at);

-- Index for metric name filtering
CREATE INDEX idx_build_metrics_name ON build_metrics(metric_name);

-- Prevent duplicate metrics for same build+name
ALTER TABLE build_metrics ADD CONSTRAINT unique_build_metric_name UNIQUE (build_id, metric_name);

-- Create view for aggregate build statistics
CREATE VIEW build_metrics_summary AS
SELECT
    b.id as build_id,
    b.job_name,
    b.status,
    b.system,
    e.jobset_id,
    j.project_id,
    b.started_at,
    b.completed_at,
    EXTRACT(EPOCH FROM (b.completed_at - b.started_at)) as duration_seconds,
    MAX(CASE WHEN bm.metric_name = 'output_size_bytes' THEN bm.metric_value END) as output_size_bytes,
    MAX(CASE WHEN bm.metric_name = 'peak_memory_bytes' THEN bm.metric_value END) as peak_memory_bytes,
    MAX(CASE WHEN bm.metric_name = 'nar_size_bytes' THEN bm.metric_value END) as nar_size_bytes
FROM builds b
JOIN evaluations e ON b.evaluation_id = e.id
JOIN jobsets j ON e.jobset_id = j.id
LEFT JOIN build_metrics bm ON b.id = bm.build_id
GROUP BY b.id, b.job_name, b.status, b.system, e.jobset_id, j.project_id, b.started_at, b.completed_at;

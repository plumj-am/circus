-- Failed paths cache: prevents rebuilding known-failing derivations
CREATE TABLE failed_paths_cache (
    drv_path TEXT PRIMARY KEY,
    source_build_id UUID,
    failure_status TEXT,
    failed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_failed_paths_cache_failed_at ON failed_paths_cache(failed_at);

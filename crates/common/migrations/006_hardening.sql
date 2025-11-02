-- Hardening: indexes for performance

-- Cache lookup index (prefix match on path)
CREATE INDEX IF NOT EXISTS idx_build_products_path_prefix ON build_products (path text_pattern_ops);

-- Composite index for pending builds query
CREATE INDEX IF NOT EXISTS idx_builds_pending_priority ON builds (status, priority DESC, created_at ASC)
    WHERE status = 'pending';

-- System filtering index
CREATE INDEX IF NOT EXISTS idx_builds_system ON builds(system) WHERE system IS NOT NULL;

-- Deduplication lookup by drv_path + status
CREATE INDEX IF NOT EXISTS idx_builds_drv_completed ON builds(drv_path) WHERE status = 'completed';

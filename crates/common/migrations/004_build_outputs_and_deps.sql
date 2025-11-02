ALTER TABLE builds ADD COLUMN outputs JSONB;
ALTER TABLE builds ADD COLUMN is_aggregate BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE builds ADD COLUMN constituents JSONB;

CREATE TABLE build_dependencies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    build_id UUID NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    dependency_build_id UUID NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    UNIQUE(build_id, dependency_build_id)
);

CREATE INDEX idx_build_deps_build ON build_dependencies(build_id);
CREATE INDEX idx_build_deps_dep ON build_dependencies(dependency_build_id);
CREATE INDEX idx_builds_drv_path ON builds(drv_path);

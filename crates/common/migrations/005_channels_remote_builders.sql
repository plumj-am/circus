-- Channels for release management (like Hydra channels)
-- A channel tracks the latest "good" evaluation for a jobset
CREATE TABLE channels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    jobset_id UUID NOT NULL REFERENCES jobsets(id) ON DELETE CASCADE,
    current_evaluation_id UUID REFERENCES evaluations(id),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, name)
);

-- Remote builders for multi-machine / multi-arch builds
CREATE TABLE remote_builders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL UNIQUE,
    ssh_uri TEXT NOT NULL,
    systems TEXT[] NOT NULL DEFAULT '{}',
    max_jobs INTEGER NOT NULL DEFAULT 1,
    speed_factor INTEGER NOT NULL DEFAULT 1,
    supported_features TEXT[] NOT NULL DEFAULT '{}',
    mandatory_features TEXT[] NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    public_host_key TEXT,
    ssh_key_file TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Track input hash for evaluation caching (skip re-eval when inputs unchanged)
ALTER TABLE evaluations ADD COLUMN inputs_hash VARCHAR(128);

-- Track which remote builder was used for a build
ALTER TABLE builds ADD COLUMN builder_id UUID REFERENCES remote_builders(id);

-- Track whether build outputs have been signed
ALTER TABLE builds ADD COLUMN signed BOOLEAN NOT NULL DEFAULT false;

-- Indexes
CREATE INDEX idx_channels_project ON channels(project_id);
CREATE INDEX idx_channels_jobset ON channels(jobset_id);
CREATE INDEX idx_remote_builders_enabled ON remote_builders(enabled) WHERE enabled = true;
CREATE INDEX idx_evaluations_inputs_hash ON evaluations(jobset_id, inputs_hash);
CREATE INDEX idx_builds_builder ON builds(builder_id) WHERE builder_id IS NOT NULL;

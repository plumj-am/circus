-- Production features: auth, priority, retry, notifications, GC roots, log paths

-- API key authentication
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(128) NOT NULL UNIQUE,
    role VARCHAR(50) NOT NULL DEFAULT 'admin'
        CHECK (role IN ('admin', 'create-projects', 'restart-jobs', 'cancel-build', 'bump-to-front', 'eval-jobset', 'read-only')),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE
);

-- Build priority and retry support
ALTER TABLE builds ADD COLUMN priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE builds ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE builds ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3;
ALTER TABLE builds ADD COLUMN notification_pending_since TIMESTAMP WITH TIME ZONE;

-- GC root tracking on build products
ALTER TABLE build_products ADD COLUMN gc_root_path TEXT;

-- Build log file path (filesystem path to captured log)
ALTER TABLE builds ADD COLUMN log_url TEXT;

-- Webhook configuration for incoming push events
CREATE TABLE webhook_configs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    forge_type VARCHAR(50) NOT NULL CHECK (forge_type IN ('github', 'gitea', 'forgejo', 'gitlab')),
    secret_hash VARCHAR(128),
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, forge_type)
);

-- Notification configuration per project
CREATE TABLE notification_configs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    notification_type VARCHAR(50) NOT NULL
        CHECK (notification_type IN ('github_status', 'gitea_status', 'forgejo_status', 'gitlab_status', 'run_command', 'email')),
    config JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, notification_type)
);

-- Jobset inputs for multi-input support
CREATE TABLE jobset_inputs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    jobset_id UUID NOT NULL REFERENCES jobsets(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    input_type VARCHAR(50) NOT NULL
        CHECK (input_type IN ('git', 'string', 'boolean', 'path', 'build')),
    value TEXT NOT NULL,
    revision TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(jobset_id, name)
);

-- Track flake mode per jobset
ALTER TABLE jobsets ADD COLUMN flake_mode BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE jobsets ADD COLUMN check_interval INTEGER NOT NULL DEFAULT 60;

-- Store the flake URI or legacy expression path in nix_expression (already exists)
-- For flake mode: nix_expression = "github:owner/repo" or "."
-- For legacy mode: nix_expression = "release.nix"

-- Indexes for new columns
CREATE INDEX idx_builds_priority ON builds(priority DESC, created_at ASC);
CREATE INDEX idx_builds_notification_pending ON builds(notification_pending_since) WHERE notification_pending_since IS NOT NULL;
CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);
CREATE INDEX idx_webhook_configs_project ON webhook_configs(project_id);
CREATE INDEX idx_notification_configs_project ON notification_configs(project_id);
CREATE INDEX idx_jobset_inputs_jobset ON jobset_inputs(jobset_id);

-- Update active_jobsets view to include flake_mode
-- Must DROP first: adding columns to jobsets changes j.* expansion,
-- and CREATE OR REPLACE VIEW cannot rename existing columns.
DROP VIEW IF EXISTS active_jobsets;
CREATE VIEW active_jobsets AS
SELECT
    j.*,
    p.name as project_name,
    p.repository_url
FROM jobsets j
JOIN projects p ON j.project_id = p.id
WHERE j.enabled = true;

-- Update list_pending to respect priority ordering
-- (handled in application code, but index above supports it)

-- Initial schema for FC
-- Creates all core tables for the CI system

-- Enable UUID extension for UUID generation
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Projects: stores repository configurations
CREATE TABLE projects (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    repository_url TEXT NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Jobsets: Contains build configurations for each project
CREATE TABLE jobsets (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    nix_expression TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, name)
);

-- Evaluations: Tracks Nix evaluation results for each jobset
CREATE TABLE evaluations (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    jobset_id UUID NOT NULL REFERENCES jobsets(id) ON DELETE CASCADE,
    commit_hash VARCHAR(40) NOT NULL,
    evaluation_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed')),
    error_message TEXT,
    UNIQUE(jobset_id, commit_hash)
);

-- Builds: Individual build jobs with their status
CREATE TABLE builds (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    evaluation_id UUID NOT NULL REFERENCES evaluations(id) ON DELETE CASCADE,
    job_name VARCHAR(255) NOT NULL,
    drv_path TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    log_path TEXT,
    build_output_path TEXT,
    error_message TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(evaluation_id, job_name)
);

-- Build products: Stores output artifacts and metadata
CREATE TABLE build_products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    build_id UUID NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    path TEXT NOT NULL,
    sha256_hash VARCHAR(64),
    file_size BIGINT,
    content_type VARCHAR(100),
    is_directory BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Build steps: Detailed build execution logs and timing
CREATE TABLE build_steps (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    build_id UUID NOT NULL REFERENCES builds(id) ON DELETE CASCADE,
    step_number INTEGER NOT NULL,
    command TEXT NOT NULL,
    output TEXT,
    error_output TEXT,
    started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE,
    exit_code INTEGER,
    UNIQUE(build_id, step_number)
);

-- Projects indexes
CREATE INDEX idx_projects_name ON projects(name);
CREATE INDEX idx_projects_created_at ON projects(created_at);

-- Jobsets indexes
CREATE INDEX idx_jobsets_project_id ON jobsets(project_id);
CREATE INDEX idx_jobsets_enabled ON jobsets(enabled);
CREATE INDEX idx_jobsets_name ON jobsets(name);

-- Evaluations indexes
CREATE INDEX idx_evaluations_jobset_id ON evaluations(jobset_id);
CREATE INDEX idx_evaluations_commit_hash ON evaluations(commit_hash);
CREATE INDEX idx_evaluations_status ON evaluations(status);
CREATE INDEX idx_evaluations_evaluation_time ON evaluations(evaluation_time);

-- Builds indexes
CREATE INDEX idx_builds_evaluation_id ON builds(evaluation_id);
CREATE INDEX idx_builds_status ON builds(status);
CREATE INDEX idx_builds_job_name ON builds(job_name);
CREATE INDEX idx_builds_started_at ON builds(started_at);
CREATE INDEX idx_builds_completed_at ON builds(completed_at);

-- Build products indexes
CREATE INDEX idx_build_products_build_id ON build_products(build_id);
CREATE INDEX idx_build_products_name ON build_products(name);

-- Build steps indexes
CREATE INDEX idx_build_steps_build_id ON build_steps(build_id);
CREATE INDEX idx_build_steps_started_at ON build_steps(started_at);

-- Create trigger functions for updated_at timestamps
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Create triggers for automatic updated_at updates
CREATE TRIGGER update_projects_updated_at
    BEFORE UPDATE ON projects
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_jobsets_updated_at
    BEFORE UPDATE ON jobsets
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Create view for active jobsets (jobsets that are enabled and belong to active projects)
CREATE VIEW active_jobsets AS
SELECT
    j.*,
    p.name as project_name,
    p.repository_url
FROM jobsets j
JOIN projects p ON j.project_id = p.id
WHERE j.enabled = true;

-- Create view for build statistics
CREATE VIEW build_stats AS
SELECT
    COUNT(*) as total_builds,
    COUNT(CASE WHEN status = 'completed' THEN 1 END) as completed_builds,
    COUNT(CASE WHEN status = 'failed' THEN 1 END) as failed_builds,
    COUNT(CASE WHEN status = 'running' THEN 1 END) as running_builds,
    COUNT(CASE WHEN status = 'pending' THEN 1 END) as pending_builds,
    AVG(EXTRACT(EPOCH FROM (completed_at - started_at))) as avg_duration_seconds
FROM builds
WHERE started_at IS NOT NULL;

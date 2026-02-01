-- Migration 008: User Management Core
-- Adds user accounts, starred jobs, and project membership tables

-- User accounts for authentication and personalization
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(255) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    full_name VARCHAR(255),
    password_hash VARCHAR(255), -- NULL for OAuth-only users
    user_type VARCHAR(50) NOT NULL DEFAULT 'local', -- 'local', 'github', 'google'
    role VARCHAR(50) NOT NULL DEFAULT 'read-only',
    enabled BOOLEAN NOT NULL DEFAULT true,
    email_verified BOOLEAN NOT NULL DEFAULT false,
    public_dashboard BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_login_at TIMESTAMP WITH TIME ZONE
);

-- Link API keys to users for audit trail
ALTER TABLE api_keys ADD COLUMN user_id UUID REFERENCES users(id) ON DELETE SET NULL;

-- Starred jobs for personalized dashboard
CREATE TABLE starred_jobs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    jobset_id UUID REFERENCES jobsets(id) ON DELETE CASCADE,
    job_name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(user_id, project_id, jobset_id, job_name)
);

-- User sessions for persistent authentication across restarts
CREATE TABLE user_sessions (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_token_hash VARCHAR(255) NOT NULL, -- Hashed session token
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMP WITH TIME ZONE
);

-- Project membership for per-project permissions
CREATE TABLE project_members (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    project_id UUID NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL DEFAULT 'member', -- 'member', 'maintainer', 'admin'
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE(project_id, user_id)
);

-- Indexes for performance
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_users_enabled ON users(enabled);
CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_starred_jobs_user_id ON starred_jobs(user_id);
CREATE INDEX idx_starred_jobs_project_id ON starred_jobs(project_id);
CREATE INDEX idx_user_sessions_token ON user_sessions(session_token_hash);
CREATE INDEX idx_user_sessions_user_id ON user_sessions(user_id);
CREATE INDEX idx_user_sessions_expires ON user_sessions(expires_at);
CREATE INDEX idx_project_members_project_id ON project_members(project_id);
CREATE INDEX idx_project_members_user_id ON project_members(user_id);

-- Trigger for updated_at on users
CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

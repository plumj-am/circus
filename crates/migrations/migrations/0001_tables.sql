-- circus database schema - tables
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- projects: stores repository configurations
CREATE TABLE projects (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  name VARCHAR(255) NOT NULL UNIQUE,
  description TEXT,
  repository_url TEXT NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- users: accounts for authentication and personalization
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  username VARCHAR(255) NOT NULL UNIQUE,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(255),
  password_hash VARCHAR(255),
  user_type VARCHAR(50) NOT NULL DEFAULT 'local',
  role VARCHAR(50) NOT NULL DEFAULT 'read-only',
  enabled BOOLEAN NOT NULL DEFAULT true,
  email_verified BOOLEAN NOT NULL DEFAULT false,
  public_dashboard BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_login_at TIMESTAMP WITH TIME ZONE
);

-- remote_builders: multi-machine / multi-arch build agents
CREATE TABLE remote_builders (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
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
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  disabled_until TIMESTAMP WITH TIME ZONE,
  last_failure TIMESTAMP WITH TIME ZONE,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- jobsets: build configurations for each project
CREATE TABLE jobsets (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  nix_expression TEXT NOT NULL,
  enabled BOOLEAN NOT NULL DEFAULT true,
  flake_mode BOOLEAN NOT NULL DEFAULT true,
  check_interval INTEGER NOT NULL DEFAULT 60,
  branch VARCHAR(255),
  scheduling_shares INTEGER NOT NULL DEFAULT 100,
  state VARCHAR(50) NOT NULL DEFAULT 'enabled' CHECK (
    state IN (
      'disabled',
      'enabled',
      'one_shot',
      'one_at_a_time'
    )
  ),
  last_checked_at TIMESTAMP WITH TIME ZONE,
  keep_nr INTEGER NOT NULL DEFAULT 3,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, name)
);

-- api_keys: authentication tokens with role-based access control
CREATE TABLE api_keys (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  name VARCHAR(255) NOT NULL,
  key_hash VARCHAR(128) NOT NULL UNIQUE,
  role VARCHAR(50) NOT NULL DEFAULT 'read-only' CHECK (
    role IN (
      'admin',
      'create-projects',
      'restart-jobs',
      'cancel-build',
      'bump-to-front',
      'eval-jobset',
      'read-only'
    )
  ),
  user_id UUID REFERENCES users (id) ON DELETE SET NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMP WITH TIME ZONE
);

-- evaluations: Nix evaluation results for each jobset commit
CREATE TABLE evaluations (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  commit_hash VARCHAR(40) NOT NULL,
  evaluation_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  status TEXT NOT NULL CHECK (
    status IN ('pending', 'running', 'completed', 'failed')
  ),
  error_message TEXT,
  inputs_hash VARCHAR(128),
  pr_number INTEGER,
  pr_head_branch TEXT,
  pr_base_branch TEXT,
  pr_action TEXT,
  UNIQUE (jobset_id, commit_hash)
);

-- builds: individual build jobs
CREATE TABLE builds (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  evaluation_id UUID NOT NULL REFERENCES evaluations (id) ON DELETE CASCADE,
  job_name VARCHAR(255) NOT NULL,
  drv_path TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
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
  ),
  started_at TIMESTAMP WITH TIME ZONE,
  completed_at TIMESTAMP WITH TIME ZONE,
  log_path TEXT,
  build_output_path TEXT,
  error_message TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  retry_count INTEGER NOT NULL DEFAULT 0,
  max_retries INTEGER NOT NULL DEFAULT 3,
  notification_pending_since TIMESTAMP WITH TIME ZONE,
  log_url TEXT,
  outputs JSONB,
  is_aggregate BOOLEAN NOT NULL DEFAULT false,
  constituents JSONB,
  builder_id UUID REFERENCES remote_builders (id),
  signed BOOLEAN NOT NULL DEFAULT false,
  system VARCHAR(50),
  keep BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (evaluation_id, job_name)
);

-- build_outputs: normalized output storage
CREATE TABLE build_outputs (
  build UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  path TEXT,
  PRIMARY KEY (build, name)
);

-- build_products: output artifacts and metadata
CREATE TABLE build_products (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  path TEXT NOT NULL,
  sha256_hash VARCHAR(64),
  file_size BIGINT,
  content_type VARCHAR(100),
  is_directory BOOLEAN NOT NULL DEFAULT false,
  gc_root_path TEXT,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- build_steps: detailed build execution logs and timing
CREATE TABLE build_steps (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  step_number INTEGER NOT NULL,
  command TEXT NOT NULL,
  output TEXT,
  error_output TEXT,
  started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMP WITH TIME ZONE,
  exit_code INTEGER,
  UNIQUE (build_id, step_number)
);

-- build_dependencies: tracks inter-build dependency relationships
CREATE TABLE build_dependencies (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  dependency_build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  UNIQUE (build_id, dependency_build_id)
);

-- webhook_configs: incoming push event configuration per project
CREATE TABLE webhook_configs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  forge_type VARCHAR(50) NOT NULL CHECK (
    forge_type IN ('github', 'gitea', 'forgejo', 'gitlab')
  ),
  secret_hash VARCHAR(128),
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, forge_type)
);

-- notification_configs: outgoing notification configuration per project
CREATE TABLE notification_configs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  notification_type VARCHAR(50) NOT NULL CHECK (
    notification_type IN (
      'github_status',
      'gitea_status',
      'forgejo_status',
      'gitlab_status',
      'webhook',
      'email'
    )
  ),
  config JSONB NOT NULL DEFAULT '{}',
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, notification_type)
);

-- jobset_inputs: parameterized inputs for jobsets
CREATE TABLE jobset_inputs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  input_type VARCHAR(50) NOT NULL CHECK (
    input_type IN ('git', 'string', 'boolean', 'path', 'build')
  ),
  value TEXT NOT NULL,
  revision TEXT,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (jobset_id, name)
);

-- channels: release management, tracks the latest good evaluation per jobset
CREATE TABLE channels (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  current_evaluation_id UUID REFERENCES evaluations (id),
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, name)
);

-- starred_jobs: personalized dashboard bookmarks per user
CREATE TABLE starred_jobs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  jobset_id UUID REFERENCES jobsets (id) ON DELETE CASCADE,
  job_name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (user_id, project_id, jobset_id, job_name)
);

-- user_sessions: persistent authentication tokens
CREATE TABLE user_sessions (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  session_token_hash VARCHAR(255) NOT NULL,
  expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMP WITH TIME ZONE
);

-- project_members: per-project permission assignments
CREATE TABLE project_members (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  role VARCHAR(50) NOT NULL DEFAULT 'member',
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, user_id)
);

-- build_metrics: timing, size, and performance metrics per build
CREATE TABLE build_metrics (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  metric_name VARCHAR(100) NOT NULL,
  metric_value DOUBLE PRECISION NOT NULL,
  unit VARCHAR(50) NOT NULL,
  collected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (build_id, metric_name)
);

-- failed_paths_cache: prevents rebuilding known-failing derivations
CREATE TABLE failed_paths_cache (
  drv_path TEXT PRIMARY KEY,
  source_build_id UUID,
  failure_status TEXT,
  failed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- notification_tasks: persistent notification retry queue
-- Stores notification delivery tasks with automatic retry and exponential backoff
CREATE TABLE notification_tasks (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  notification_type VARCHAR(50) NOT NULL CHECK (
    notification_type IN (
      'webhook',
      'github_status',
      'gitea_status',
      'gitlab_status',
      'email'
    )
  ),
  payload JSONB NOT NULL,
  status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (
    status IN ('pending', 'running', 'completed', 'failed')
  ),
  attempts INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 5,
  next_retry_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_error TEXT,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMP WITH TIME ZONE
);

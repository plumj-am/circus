-- circus database schema - indexes
-- Indexes: projects
CREATE INDEX idx_projects_name ON projects (name);

CREATE INDEX idx_projects_created_at ON projects (created_at);

-- Indexes: users
CREATE INDEX idx_users_username ON users (username);

CREATE INDEX idx_users_email ON users (email);

CREATE INDEX idx_users_role ON users (role);

CREATE INDEX idx_users_enabled ON users (enabled);

-- Indexes: remote_builders
CREATE INDEX idx_remote_builders_enabled ON remote_builders (enabled)
WHERE
  enabled = true;

-- Indexes: jobsets
CREATE INDEX idx_jobsets_project_id ON jobsets (project_id);

CREATE INDEX idx_jobsets_enabled ON jobsets (enabled);

CREATE INDEX idx_jobsets_name ON jobsets (name);

CREATE INDEX idx_jobsets_state ON jobsets (state);

CREATE INDEX idx_jobsets_last_checked_at ON jobsets (last_checked_at);

-- Indexes: api_keys
CREATE INDEX idx_api_keys_key_hash ON api_keys (key_hash);

CREATE INDEX idx_api_keys_user_id ON api_keys (user_id);

-- Indexes: evaluations
CREATE INDEX idx_evaluations_jobset_id ON evaluations (jobset_id);

CREATE INDEX idx_evaluations_commit_hash ON evaluations (commit_hash);

CREATE INDEX idx_evaluations_status ON evaluations (status);

CREATE INDEX idx_evaluations_evaluation_time ON evaluations (evaluation_time);

CREATE INDEX idx_evaluations_inputs_hash ON evaluations (jobset_id, inputs_hash);

CREATE INDEX idx_evaluations_pr ON evaluations (jobset_id, pr_number)
WHERE
  pr_number IS NOT NULL;

-- Indexes: builds
CREATE INDEX idx_builds_evaluation_id ON builds (evaluation_id);

CREATE INDEX idx_builds_status ON builds (status);

CREATE INDEX idx_builds_job_name ON builds (job_name);

CREATE INDEX idx_builds_started_at ON builds (started_at);

CREATE INDEX idx_builds_completed_at ON builds (completed_at);

CREATE INDEX idx_builds_priority ON builds (priority DESC, created_at ASC);

CREATE INDEX idx_builds_notification_pending ON builds (notification_pending_since)
WHERE
  notification_pending_since IS NOT NULL;

CREATE INDEX idx_builds_drv_path ON builds (drv_path);

CREATE INDEX idx_builds_builder ON builds (builder_id)
WHERE
  builder_id IS NOT NULL;

CREATE INDEX idx_builds_system ON builds (system)
WHERE
  system IS NOT NULL;

CREATE INDEX idx_builds_pending_priority ON builds (status, priority DESC, created_at ASC)
WHERE
  status = 'pending';

CREATE INDEX idx_builds_drv_completed ON builds (drv_path)
WHERE
  status = 'succeeded';

-- Indexes: build_outputs
CREATE INDEX idx_build_outputs_path ON build_outputs USING btree (path);

-- Indexes: build_products
CREATE INDEX idx_build_products_build_id ON build_products (build_id);

CREATE INDEX idx_build_products_name ON build_products (name);

CREATE INDEX idx_build_products_path_prefix ON build_products (path text_pattern_ops);

-- Indexes: build_steps
CREATE INDEX idx_build_steps_build_id ON build_steps (build_id);

CREATE INDEX idx_build_steps_started_at ON build_steps (started_at);

-- Indexes: build_dependencies
CREATE INDEX idx_build_deps_build ON build_dependencies (build_id);

CREATE INDEX idx_build_deps_dep ON build_dependencies (dependency_build_id);

-- Indexes: webhook/notification/jobset_inputs/channels
CREATE INDEX idx_webhook_configs_project ON webhook_configs (project_id);

CREATE INDEX idx_notification_configs_project ON notification_configs (project_id);

CREATE INDEX idx_jobset_inputs_jobset ON jobset_inputs (jobset_id);

CREATE INDEX idx_channels_project ON channels (project_id);

CREATE INDEX idx_channels_jobset ON channels (jobset_id);

-- Indexes: users/sessions/members
CREATE INDEX idx_starred_jobs_user_id ON starred_jobs (user_id);

CREATE INDEX idx_starred_jobs_project_id ON starred_jobs (project_id);

CREATE INDEX idx_user_sessions_token ON user_sessions (session_token_hash);

CREATE INDEX idx_user_sessions_user_id ON user_sessions (user_id);

CREATE INDEX idx_user_sessions_expires ON user_sessions (expires_at);

CREATE INDEX idx_project_members_project_id ON project_members (project_id);

CREATE INDEX idx_project_members_user_id ON project_members (user_id);

-- Indexes: build_metrics / failed_paths_cache
CREATE INDEX idx_build_metrics_build_id ON build_metrics (build_id);

CREATE INDEX idx_build_metrics_collected_at ON build_metrics (collected_at);

CREATE INDEX idx_build_metrics_name ON build_metrics (metric_name);

CREATE INDEX idx_failed_paths_cache_failed_at ON failed_paths_cache (failed_at);

-- Indexes: notification_tasks
CREATE INDEX idx_notification_tasks_status_next_retry ON notification_tasks (status, next_retry_at)
WHERE
  status IN ('pending', 'running');

CREATE INDEX idx_notification_tasks_created_at ON notification_tasks (created_at);

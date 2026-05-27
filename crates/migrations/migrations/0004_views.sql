-- circus database schema - views
CREATE VIEW active_jobsets AS
SELECT
  j.id,
  j.project_id,
  j.name,
  j.nix_expression,
  j.enabled,
  j.flake_mode,
  j.check_interval,
  j.branch,
  j.scheduling_shares,
  j.created_at,
  j.updated_at,
  j.state,
  j.last_checked_at,
  j.keep_nr,
  p.name as project_name,
  p.repository_url
FROM
  jobsets j
  JOIN projects p ON j.project_id = p.id
WHERE
  j.state IN ('enabled', 'one_shot', 'one_at_a_time');

CREATE VIEW build_stats AS
SELECT
  COUNT(*) as total_builds,
  COUNT(
    CASE
      WHEN status = 'succeeded' THEN 1
    END
  ) as completed_builds,
  COUNT(
    CASE
      WHEN status = 'failed' THEN 1
    END
  ) as failed_builds,
  COUNT(
    CASE
      WHEN status = 'running' THEN 1
    END
  ) as running_builds,
  COUNT(
    CASE
      WHEN status = 'pending' THEN 1
    END
  ) as pending_builds,
  AVG(
    EXTRACT(
      EPOCH
      FROM
        (completed_at - started_at)
    )
  )::double precision as avg_duration_seconds
FROM
  builds
WHERE
  started_at IS NOT NULL;

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
  EXTRACT(
    EPOCH
    FROM
      (b.completed_at - b.started_at)
  ) as duration_seconds,
  MAX(
    CASE
      WHEN bm.metric_name = 'output_size_bytes' THEN bm.metric_value
    END
  ) as output_size_bytes,
  MAX(
    CASE
      WHEN bm.metric_name = 'peak_memory_bytes' THEN bm.metric_value
    END
  ) as peak_memory_bytes,
  MAX(
    CASE
      WHEN bm.metric_name = 'nar_size_bytes' THEN bm.metric_value
    END
  ) as nar_size_bytes
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
  LEFT JOIN build_metrics bm ON b.id = bm.build_id
GROUP BY
  b.id,
  b.job_name,
  b.status,
  b.system,
  e.jobset_id,
  j.project_id,
  b.started_at,
  b.completed_at;

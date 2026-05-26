-- The active_jobsets view previously filtered only by `state`, making the
-- `enabled` boolean column a no-op for evaluation gating. Both `upsert` and
-- `update` in the jobsets repo already sync `enabled` from `state.is_evaluable()`
-- when state is explicitly set, so adding this filter is consistent with the
-- existing write-path semantics. It also honours the case where a jobset is
-- disabled via the API by setting `enabled = false` without touching `state`.
CREATE OR REPLACE VIEW active_jobsets AS
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
  p.name AS project_name,
  p.repository_url AS repository_url
FROM
  jobsets j
  JOIN projects p ON j.project_id = p.id
WHERE
  j.state IN ('enabled', 'one_shot', 'one_at_a_time')
  AND j.enabled = true;

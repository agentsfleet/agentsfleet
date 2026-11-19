WITH owned AS MATERIALIZED (
  SELECT l.created_at, l.run_expires_at, l.tenant_id, l.workspace_id, l.fleet_id, l.event_id
  FROM thread.runs l JOIN thread.turns e ON e.id = l.event_id AND e.thread_id = l.fleet_id
  WHERE l.id = $1 AND l.runner_id = $2
  FOR UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the run lock is held, so a lock wait cannot make the slice end
  -- before the moment this statement owns the run.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM owned) locked
)
SELECT 0::bigint AS runtime_seq, LEAST(i.checked_at, o.run_expires_at) AS checked_at,
       o.created_at AS started_at, 0::bigint AS d_ms, o.tenant_id,
       o.workspace_id, o.fleet_id, f.name AS fleet_name, o.event_id
FROM owned o
CROSS JOIN instant i
LEFT JOIN fleet.fleets f ON f.id = o.fleet_id AND f.workspace_id = o.workspace_id

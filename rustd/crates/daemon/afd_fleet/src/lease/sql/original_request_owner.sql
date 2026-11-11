WITH owned AS MATERIALIZED (
  SELECT l.created_at, l.lease_expires_at, l.tenant_id, l.workspace_id, l.fleet_id, l.event_id
  FROM runner.leases l JOIN fleet.events e ON e.event_id = l.event_id
  WHERE l.id = $1 AND l.runner_id = $2
  FOR UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the lease lock is held, so a lock wait cannot make the slice end
  -- before the moment this statement owns the lease.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM owned) locked
)
SELECT 0::bigint AS runtime_seq, LEAST(i.checked_at, o.lease_expires_at) AS checked_at,
       o.created_at AS started_at, 0::bigint AS d_ms, o.tenant_id,
       o.workspace_id, o.fleet_id, f.name AS fleet_name, o.event_id
FROM owned o
CROSS JOIN instant i
LEFT JOIN fleet.fleets f ON f.id = o.fleet_id AND f.workspace_id = o.workspace_id

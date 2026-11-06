SELECT 0::bigint AS daemon_lines, LEAST(clock_timestamp(), l.lease_expires_at) AS checked_at,
       l.created_at AS started_at, 0::bigint AS d_ms, l.tenant_id::text,
       l.workspace_id::text, l.fleet_id::text, f.name AS fleet_name, e.event_id
FROM runner.leases l JOIN fleet.events e ON e.id = l.fleet_event_id
LEFT JOIN fleet.fleets f ON f.id = l.fleet_id AND f.workspace_id = l.workspace_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
FOR UPDATE OF l

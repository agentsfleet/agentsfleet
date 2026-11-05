INSERT INTO runner.fleet_slots AS s
  (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
   ready_at, created_at, updated_at)
SELECT DISTINCT g.fleet_id, NULL::uuid, 0, clock_timestamp() - interval '1 millisecond',
       NULL::timestamptz, NULL::timestamptz, now(), now()
FROM fleet.approval_gates g
WHERE g.resolved_at IS NULL AND g.expires_at <= now()
ORDER BY g.fleet_id
ON CONFLICT (fleet_id) DO UPDATE SET updated_at = s.updated_at
RETURNING s.fleet_id::text AS fleet_id

INSERT INTO runner.fleet_slots AS s
  (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
   ready_at, created_at, updated_at)
SELECT f.fleet_id, NULL::uuid, 0, to_timestamp(0), NULL::timestamptz, NULL::timestamptz,
       now(), now()
FROM (SELECT DISTINCT g.fleet_id FROM fleet.approval_gates g
WHERE g.action_id = $1 AND g.resolved_at IS NULL
  AND ($2::text = '' OR g.fleet_id::text = $2)) f
ORDER BY f.fleet_id
ON CONFLICT (fleet_id) DO UPDATE SET updated_at = s.updated_at
RETURNING s.fleet_id::text AS fleet_id

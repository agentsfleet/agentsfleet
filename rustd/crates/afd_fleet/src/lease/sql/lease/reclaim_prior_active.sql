WITH bumped AS (
  UPDATE runner.leases AS l
  SET status = $3, updated_at = now()
  WHERE l.id = (
      SELECT id FROM runner.leases
      WHERE fleet_id = $1::uuid AND status = $2
      ORDER BY fencing_token DESC LIMIT 1
      FOR UPDATE
  )
  RETURNING l.id, l.runner_id, l.fleet_event_id, l.receipt, l.event_created_at,
            l.workspace_id, l.tenant_id, l.posture, l.model
), tally AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT runner_id, 0, 0, 0, 1, now(), now()
  FROM bumped
  ON CONFLICT (runner_id) DO UPDATE
     SET expired_count = c.expired_count + 1, updated_at = now()
)
SELECT b.id::text AS lease_id, e.event_id, e.actor, e.event_type,
       e.request_json::text AS request_json, b.event_created_at,
       b.workspace_id::text AS workspace_id, b.tenant_id::text AS tenant_id,
       b.posture, b.model, b.receipt
FROM bumped b
JOIN fleet.events e ON e.id = b.fleet_event_id

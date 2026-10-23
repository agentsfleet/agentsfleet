WITH locked_slot AS MATERIALIZED (
  SELECT s.fleet_id FROM runner.fleet_slots s
  WHERE s.fleet_id = $1::uuid
  FOR UPDATE OF s SKIP LOCKED
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM locked_slot) locked
), slot AS (
  INSERT INTO runner.fleet_slots AS s
    (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
     created_at, updated_at)
  SELECT $1::uuid, $2::uuid, 1, instant.checked_at + $3::interval, NULL, now(), now()
  FROM instant
  WHERE EXISTS (SELECT 1 FROM locked_slot)
     OR NOT EXISTS (SELECT 1 FROM runner.fleet_slots WHERE fleet_id = $1::uuid)
  ON CONFLICT (fleet_id) DO UPDATE
    SET last_runner_id = EXCLUDED.last_runner_id,
        fencing_token = s.fencing_token + 1,
        lease_expires_at = clock_timestamp() + $3::interval,
        hold_expires_at = CASE WHEN s.last_runner_id = EXCLUDED.last_runner_id
                               THEN s.hold_expires_at END,
        updated_at = now()
    WHERE s.lease_expires_at < (SELECT checked_at FROM instant)
      AND (s.hold_expires_at IS NULL
           OR s.hold_expires_at <= (SELECT checked_at FROM instant)
           OR s.last_runner_id = $2::uuid
           OR NOT EXISTS (
             SELECT 1 FROM runner.runners r
             WHERE r.id = s.last_runner_id
               AND r.last_seen_at > (SELECT checked_at FROM instant) - $4::interval
               AND r.status = $5 AND r.degraded_reason IS NULL))
  RETURNING WITH (OLD AS prior) s.fencing_token, s.lease_expires_at,
            prior.hold_expires_at > (SELECT checked_at FROM instant) AS held,
            prior.last_runner_id::text AS holder
), head AS MATERIALIZED (
  SELECT a.id FROM fleet.admissions a
  WHERE a.fleet_id = $1::uuid AND a.settled_at IS NULL
  ORDER BY a.sequence
  LIMIT 1
), locked AS (
  SELECT a.id FROM fleet.admissions a
  JOIN head h ON h.id = a.id
  CROSS JOIN slot
  WHERE a.settled_at IS NULL
  FOR UPDATE OF a SKIP LOCKED
), stamped AS (
  UPDATE fleet.admissions a
  SET lease_fence = slot.fencing_token, updated_at = now()
  FROM locked l, slot
  WHERE a.id = l.id AND a.settled_at IS NULL
    AND (a.parked_at IS NULL OR NOT EXISTS (
      (SELECT 1 FROM fleet.approval_gates g
      WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
        AND g.event_id IS NULL LIMIT 1)
      UNION ALL
      (SELECT 1 FROM fleet.approval_gates g
      WHERE g.fleet_id = a.fleet_id AND g.resolved_at IS NULL
        AND g.event_id = a.id::text
        AND g.expires_at > (SELECT checked_at FROM instant) LIMIT 1)))
  RETURNING a.id, a.workspace_id, a.actor, a.event_type, a.request_json,
            a.event_created_at, a.delivered_at
), prior_lease AS MATERIALIZED (
  SELECT l.id, l.runner_id, l.tenant_id, l.posture, l.model
  FROM runner.leases l JOIN fleet.events e ON e.id = l.fleet_event_id
  JOIN stamped a ON e.event_id = a.id::text
  WHERE l.fleet_id = $1::uuid AND e.fleet_id = $1::uuid
  ORDER BY l.fencing_token DESC LIMIT 1
  FOR UPDATE OF l
), bumped AS (
  UPDATE runner.leases l SET status = $7, updated_at = now()
  FROM prior_lease p WHERE l.id = p.id AND l.status = $6
  RETURNING l.runner_id
), tally AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT runner_id, 0, 0, 0, 1, now(), now() FROM bumped
  ON CONFLICT (runner_id) DO UPDATE
    SET expired_count = c.expired_count + 1, updated_at = now()
)
SELECT a.id::text AS event_id, a.workspace_id::text, a.actor, a.event_type,
       a.request_json, a.event_created_at,
       a.delivered_at IS NOT NULL AS redelivered,
       slot.fencing_token, slot.lease_expires_at,
       COALESCE(slot.held, false) AS held, slot.holder,
       b.tenant_id::text AS reused_tenant_id, b.posture AS reused_posture,
       b.model AS reused_model
FROM stamped a CROSS JOIN slot
LEFT JOIN prior_lease b ON true

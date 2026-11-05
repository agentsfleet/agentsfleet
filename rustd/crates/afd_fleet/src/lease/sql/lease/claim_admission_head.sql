WITH replay AS MATERIALIZED (
  SELECT l.id, l.fleet_id, l.fencing_token, l.status = $6 AS active
  FROM runner.leases l
  WHERE $11::uuid IS NOT NULL AND l.runner_id = $1::uuid AND l.poll_key = $11::uuid
), replay_slot AS MATERIALIZED (
  SELECT s.fleet_id FROM runner.fleet_slots s
  JOIN replay r ON r.fleet_id = s.fleet_id AND r.fencing_token = s.fencing_token
  WHERE r.active
  FOR UPDATE OF s
), replay_lease AS MATERIALIZED (
  SELECT l.id FROM runner.leases l
  JOIN replay r ON r.id = l.id
  JOIN replay_slot rs ON rs.fleet_id = l.fleet_id
  WHERE l.status = $6
  FOR UPDATE OF l
), replay_expired AS (
  UPDATE runner.leases l SET status = $7, updated_at = now()
  FROM replay_lease r WHERE l.id = r.id
  RETURNING l.id
), replay_freed AS (
  UPDATE runner.fleet_slots s
  SET lease_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now()
  FROM replay_slot r
  WHERE s.fleet_id = r.fleet_id AND EXISTS (SELECT 1 FROM replay_expired)
  RETURNING s.fleet_id
), labels AS MATERIALIZED (
  SELECT COALESCE(array_agg(e), '{}'::text[]) AS tags
  FROM jsonb_array_elements_text(
    (SELECT CASE WHEN jsonb_typeof(labels) = 'array' THEN labels ELSE '[]'::jsonb END
     FROM runner.runners WHERE id = $1::uuid)) AS e
), chosen AS MATERIALIZED (
  SELECT s.fleet_id, head.id AS head_id
  FROM runner.fleet_slots s
  CROSS JOIN LATERAL (
    SELECT z.status, z.required_tags FROM fleet.fleets z WHERE z.id = s.fleet_id LIMIT 1
  ) f
  CROSS JOIN LATERAL (
    SELECT h.id FROM fleet.admissions h
    WHERE h.fleet_id = s.fleet_id AND h.settled_at IS NULL
    ORDER BY h.sequence LIMIT 1
  ) head
  WHERE NOT EXISTS (SELECT 1 FROM replay)
    AND s.ready_at IS NOT NULL
    AND s.lease_expires_at < statement_timestamp()
    AND f.status = $8
    AND f.required_tags <@ (SELECT tags FROM labels)
    AND (s.hold_expires_at IS NULL
         OR s.hold_expires_at <= statement_timestamp()
         OR s.last_runner_id = $1::uuid
         OR NOT EXISTS (
           SELECT 1 FROM runner.runners rr
           WHERE rr.id = s.last_runner_id
             AND rr.last_seen_at > statement_timestamp() - $4::interval
             AND rr.status = $5 AND rr.degraded_reason IS NULL))
  ORDER BY s.fleet_id = ANY((SELECT ($2::text[])::uuid[])::uuid[]) DESC,
           (s.last_runner_id = $1::uuid) DESC NULLS LAST,
           s.ready_at
  LIMIT 1
  FOR UPDATE OF s SKIP LOCKED
), head AS MATERIALIZED (
  SELECT a.id, a.workspace_id, a.actor, a.event_type, a.request_json,
         a.event_created_at, a.created_at, a.producer, a.delivered_at,
         a.settled_at, a.parked_at
  FROM fleet.admissions a
  WHERE a.id = (SELECT c.head_id FROM chosen c)
  FOR UPDATE OF a SKIP LOCKED
), pick AS MATERIALIZED (
  SELECT c.fleet_id, h.id AS admission_id, h.workspace_id, h.actor, h.event_type,
         h.request_json, h.event_created_at, h.created_at, h.producer, h.delivered_at
  FROM chosen c
  JOIN head h ON h.id = c.head_id
  WHERE h.settled_at IS NULL AND h.parked_at IS NULL
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at FROM pick
), slot AS (
  UPDATE runner.fleet_slots s
  SET last_runner_id = $1::uuid,
      fencing_token = s.fencing_token + 1,
      lease_expires_at = i.checked_at + $3::interval,
      hold_expires_at = CASE WHEN s.last_runner_id = $1::uuid THEN s.hold_expires_at END,
      updated_at = now()
  FROM pick p CROSS JOIN instant i
  WHERE s.fleet_id = p.fleet_id
  RETURNING WITH (OLD AS prior) s.fleet_id, s.fencing_token, s.lease_expires_at,
            prior.hold_expires_at > i.checked_at AS held,
            prior.last_runner_id::text AS holder
), stamped AS (
  UPDATE fleet.admissions a
  SET lease_fence = slot.fencing_token,
      delivered_at = COALESCE(a.delivered_at, i.checked_at),
      updated_at = now()
  FROM pick p CROSS JOIN slot CROSS JOIN instant i
  WHERE a.id = p.admission_id
  RETURNING WITH (OLD AS prior) a.id, prior.delivered_at IS NOT NULL AS redelivered
), opened AS (
  INSERT INTO fleet.events AS e
    (fleet_id, event_id, workspace_id, actor, event_type,
     status, request_json, resumes_event_id, created_at, updated_at)
  SELECT p.fleet_id, p.admission_id::text, p.workspace_id, p.actor, p.event_type,
         $9, p.request_json::jsonb, NULL,
         CASE WHEN p.producer = $10 AND p.delivered_at IS NULL THEN p.created_at ELSE now() END,
         now()
  FROM pick p JOIN stamped st ON st.id = p.admission_id
  ON CONFLICT (fleet_id, event_id) DO UPDATE SET
    resumes_event_id = COALESCE(e.resumes_event_id, EXCLUDED.resumes_event_id)
  RETURNING (xmax = 0) AS inserted, e.status, e.created_at
), prior_lease AS MATERIALIZED (
  SELECT l.id, l.runner_id, l.tenant_id, l.posture, l.model
  FROM runner.leases l
  JOIN fleet.events e ON e.id = l.fleet_event_id
  JOIN pick p ON e.event_id = p.admission_id::text AND e.fleet_id = p.fleet_id
  WHERE l.fleet_id = p.fleet_id
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
SELECT NULL::text AS replayed_lease, false AS replay_freed,
       p.fleet_id::text AS fleet_id, p.admission_id::text AS event_id,
       p.workspace_id::text AS workspace_id, p.actor, p.event_type, p.request_json,
       p.event_created_at, st.redelivered, slot.fencing_token, slot.lease_expires_at,
       COALESCE(slot.held, false) AS held, slot.holder,
       b.tenant_id::text AS reused_tenant_id, b.posture AS reused_posture,
       b.model AS reused_model,
       o.inserted AS opened, o.status AS event_status, o.created_at AS opened_at
FROM pick p
JOIN slot ON slot.fleet_id = p.fleet_id
JOIN stamped st ON st.id = p.admission_id
CROSS JOIN opened o
LEFT JOIN prior_lease b ON true
UNION ALL
SELECT r.id::text, EXISTS (SELECT 1 FROM replay_freed),
       r.fleet_id::text, NULL::text, NULL::text, NULL::text, NULL::text, NULL::text,
       NULL::timestamptz, NULL::boolean, r.fencing_token, NULL::timestamptz,
       NULL::boolean, NULL::text, NULL::text, NULL::text, NULL::text,
       NULL::boolean, NULL::text, NULL::timestamptz
FROM replay r

WITH slot AS MATERIALIZED (
  SELECT s.fleet_id FROM runner.fleet_slots s
  WHERE s.fleet_id = $1::uuid AND s.fencing_token = $3
  FOR UPDATE OF s
), admission AS MATERIALIZED (
  SELECT a.id FROM fleet.admissions a
  JOIN slot s ON s.fleet_id = a.fleet_id
  WHERE a.id = $2::uuid AND a.lease_fence = $3 AND a.settled_at IS NULL
  FOR UPDATE OF a
), closed AS (
  UPDATE fleet.events e
  SET status = $4, failure_reason = $5, updated_at = now(),
      failure_detail = NULLIF($7, '')
  FROM admission a
  WHERE e.fleet_id = $1::uuid AND e.event_id = a.id::text AND e.status = $6
  RETURNING e.fleet_id, e.event_id, e.workspace_id, e.actor, e.event_type, e.status,
            e.token_count, e.wall_ms, e.failure_reason, e.failure_detail,
            e.resumes_event_id, e.created_at, e.updated_at
), settled AS (
  UPDATE fleet.admissions a
  SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
  FROM admission x WHERE a.id = x.id
  RETURNING a.fleet_id
), freed AS (
  UPDATE runner.fleet_slots s
  SET lease_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
      last_runner_id = CASE WHEN s.hold_expires_at > clock_timestamp() THEN s.last_runner_id END
  FROM settled x
  WHERE s.fleet_id = x.fleet_id
  RETURNING s.fleet_id
)
SELECT c.fleet_id::text AS fleet_id, c.event_id, c.workspace_id::text AS workspace_id,
       c.actor, c.event_type, c.status, c.token_count, c.wall_ms,
       c.failure_reason, c.failure_detail, c.resumes_event_id,
       c.created_at, c.updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.usage_lines charge
         WHERE charge.event_id = c.event_id
           AND charge.fleet_id = c.fleet_id) AS cost_nanos,
       f.status AS fleet_status,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = c.fleet_id AND g.status = $8) AS pending_approvals,
       COALESCE((SELECT k.event_count FROM fleet.activity_counters k
                  WHERE k.fleet_id = c.fleet_id), 0) AS event_count,
       COALESCE((SELECT k.budget_used_nanos FROM fleet.activity_counters k
                  WHERE k.fleet_id = c.fleet_id), 0) AS budget_used_nanos
FROM closed c
JOIN fleet.fleets f ON f.id = c.fleet_id

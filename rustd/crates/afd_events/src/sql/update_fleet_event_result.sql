WITH closed AS (
  UPDATE fleet.events
  SET status = $3, response_text = $4, token_count = $5, wall_ms = $6, updated_at = now(),
      failure_reason = $7, failure_detail = $9, tool_calls = $11::jsonb
  WHERE fleet_id = $1::uuid AND event_id = $2 AND status = $8
  RETURNING fleet_id, event_id, workspace_id, actor, event_type, status,
            token_count, wall_ms, failure_reason, failure_detail,
            resumes_event_id, created_at, updated_at
)
SELECT c.fleet_id::text AS fleet_id, c.event_id, c.workspace_id::text AS workspace_id,
       c.actor, c.event_type, c.status, c.token_count, c.wall_ms,
       c.failure_reason, c.failure_detail, c.resumes_event_id,
       c.created_at, c.updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.fleet_charges charge
         WHERE charge.event_id = c.event_id
           AND charge.fleet_id = c.fleet_id) AS cost_nanos,
       f.status AS fleet_status,
       (SELECT COUNT(*) FROM fleet.approval_gates g
         WHERE g.fleet_id = c.fleet_id AND g.status = $10) AS pending_approvals,
       COALESCE((SELECT k.event_count FROM fleet.activity_counters k
                  WHERE k.fleet_id = c.fleet_id), 0) AS event_count,
       COALESCE((SELECT k.budget_used_nanos FROM fleet.activity_counters k
                  WHERE k.fleet_id = c.fleet_id), 0) AS budget_used_nanos
FROM closed c
JOIN fleet.fleets f ON f.id = c.fleet_id

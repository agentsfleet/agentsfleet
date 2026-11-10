-- Closes, with the run's answer, each folded message's event that is still
-- received: update_fleet_event_result.sql for many events in one statement,
-- with `$2` a list of event ids. An event already closed keeps its result.
-- Each closed row carries the fleet facts the live tail reads.
--
-- $1 fleet · $2 event ids · $3 new status · $4 response text · $5 tokens · $6 wall milliseconds ·
-- $7 failure reason · $8 the status this transition is guarded on · $9 failure detail ·
-- $10 the gate status that counts as pending · $11 tool trace JSON text, or NULL
WITH closed AS (
  UPDATE fleet.events
  SET status = $3, response_text = $4, token_count = $5, wall_ms = $6, updated_at = now(),
      failure_reason = $7, failure_detail = $9, tool_calls = $11::jsonb
  WHERE fleet_id = $1 AND event_id = ANY($2) AND status = $8
  RETURNING fleet_id, event_id, workspace_id, actor, event_type, status,
            token_count, wall_ms, failure_reason, failure_detail,
            resumes_event_id, created_at, updated_at
)
SELECT c.fleet_id AS fleet_id, c.event_id AS event_id, c.workspace_id AS workspace_id,
       c.actor, c.event_type, c.status, c.token_count, c.wall_ms,
       c.failure_reason, c.failure_detail, c.resumes_event_id AS resumes_event_id,
       c.created_at, c.updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.usage_lines charge
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
ORDER BY c.created_at, c.event_id

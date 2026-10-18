SELECT fleet_id::text AS fleet_id, event_id, workspace_id::text AS workspace_id,
       actor, event_type, status, token_count, wall_ms,
       failure_reason, failure_detail, resumes_event_id,
       created_at, updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.fleet_charges charge
         WHERE charge.event_id = fleet.events.event_id
           AND charge.fleet_id = fleet.events.fleet_id) AS cost_nanos,
       request_json::text AS request_json, response_text, tool_calls::text AS tool_calls
FROM fleet.events
WHERE workspace_id = $1::uuid
  AND fleet_id = $2::uuid
  AND (created_at, event_id) < ($3::timestamptz, $4)
ORDER BY created_at DESC, event_id DESC
LIMIT $5

(SELECT fleet_id::text AS fleet_id, event_id, workspace_id::text AS workspace_id,
       actor, event_type, status, token_count, wall_ms,
       failure_reason, failure_detail, resumes_event_id,
       created_at, updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.usage_lines charge
         WHERE charge.event_id = fleet.events.event_id
           AND charge.fleet_id = fleet.events.fleet_id) AS cost_nanos,
       request_json::text AS request_json, response_text, tool_calls::text AS tool_calls
FROM fleet.events
WHERE workspace_id = $1::uuid
  AND fleet_id = $2::uuid
  AND (created_at, event_id) < ($3::timestamptz, $4)
ORDER BY created_at DESC, event_id DESC
LIMIT $5)
UNION ALL
(SELECT a.fleet_id::text AS fleet_id, a.id::text AS event_id,
       a.workspace_id::text AS workspace_id, a.actor, a.event_type, $7::text AS status,
       NULL::bigint AS token_count, NULL::bigint AS wall_ms,
       NULL::text AS failure_reason, NULL::text AS failure_detail,
       NULL::text AS resumes_event_id, a.created_at, a.created_at AS updated_at,
       NULL::bigint AS cost_nanos, a.request_json, NULL::text AS response_text,
       NULL::text AS tool_calls
FROM fleet.admissions a
WHERE a.workspace_id = $1::uuid AND a.fleet_id = $2::uuid
  AND a.producer = $6 AND a.delivered_at IS NULL AND a.settled_at IS NULL
  AND (a.created_at, a.id::text) < ($3::timestamptz, $4)
ORDER BY a.created_at DESC, a.id DESC
LIMIT $5)
ORDER BY created_at DESC, event_id DESC
LIMIT $5

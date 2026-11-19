SELECT thread_id AS fleet_id, id AS event_id, workspace_id,
       actor, kind AS event_type, status, token_count, wall_ms,
       failure_reason, failure_detail, resumes_turn_id AS resumes_event_id,
       created_at, updated_at,
       (SELECT SUM(charge.charged_nanos)::bigint
          FROM billing.usage_lines charge
         WHERE charge.event_id = thread.turns.id
           AND charge.fleet_id = thread.turns.thread_id) AS cost_nanos
FROM thread.turns
WHERE workspace_id = $1 AND opened_at IS NOT NULL
  AND (created_at, id) < ($2::timestamptz, $3)
  AND actor LIKE $4
  AND created_at >= COALESCE($5::timestamptz, statement_timestamp() - $6::interval, '-infinity')
ORDER BY created_at DESC, id DESC
LIMIT $7

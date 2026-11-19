-- LEFT JOINs: a settled run outlives its fleet and its event (thread.runs
-- holds no foreign key to either), so a purged fleet's history still lists,
-- with the fleet and event columns null.
SELECT l.id,
       l.fleet_id,
       f.name AS fleet_name,
       f.id IS NOT NULL AS fleet_exists,
       l.workspace_id,
       l.tenant_id,
       l.event_id,
       e.event_type,
       e.actor,
       l.status AS run_status,
       l.run_expires_at,
       l.created_at,
       l.fencing_token,
       l.provider,
       l.model,
       l.posture,
       COALESCE(u.input_tokens, 0)::bigint        AS metered_input_token_count,
       COALESCE(u.cached_input_tokens, 0)::bigint AS metered_cached_token_count,
       COALESCE(u.output_tokens, 0)::bigint       AS metered_output_token_count,
       e.status AS event_status,
       e.failure_reason,
       e.failure_detail,
       e.wall_ms,
       EXISTS (
           SELECT 1 FROM thread.runs p
           WHERE p.event_id = l.event_id
             AND p.fencing_token < l.fencing_token
       ) AS is_reclaim
FROM thread.runs l
LEFT JOIN fleet.fleets f ON f.id = l.fleet_id
LEFT JOIN fleet.events e ON e.event_id = l.event_id
-- The tokens the run's usage lines record: one index range per run row.
LEFT JOIN LATERAL (
    SELECT sum(input_token_count) AS input_tokens,
           sum(cached_input_token_count) AS cached_input_tokens,
           sum(output_token_count) AS output_tokens
    FROM billing.usage_lines
    WHERE work_type = $7 AND work_id = l.id
) u ON true
WHERE l.runner_id = $1 AND l.fleet_id IS NOT NULL
  AND (l.workspace_id = $2 OR $2 IS NULL)
  AND ($3::text IS NULL OR l.fleet_id = $8 OR lower(f.name) = lower($3::text))
  AND (l.created_at, l.id) < (COALESCE($5::timestamptz, 'infinity'::timestamptz), $6)
ORDER BY l.created_at DESC, l.id DESC
LIMIT $4

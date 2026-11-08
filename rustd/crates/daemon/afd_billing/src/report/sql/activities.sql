WITH grouped AS (
    SELECT work_type, work_id,
           MIN(workspace_id::text) AS workspace_id,
           MAX(fleet_id::text) AS fleet_id, MAX(fleet_name) AS fleet_name,
           MAX(created_at) AS recorded_at,
           SUM(charged_nanos)::bigint AS charged_nanos,
           SUM(input_token_count)::bigint AS input_tokens,
           SUM(output_token_count)::bigint AS output_tokens,
           SUM(runtime_ms)::bigint AS runtime_ms,
           COUNT(*)::bigint AS line_count,
           SUM(unpriced_token_count)::bigint AS unpriced_tokens,
           ARRAY_AGG(DISTINCT model ORDER BY model) AS models,
           ARRAY_AGG(DISTINCT posture ORDER BY posture) AS postures
      FROM billing.usage_lines
     WHERE tenant_id = $1::uuid
       AND created_at >= $2 AND created_at <= $3
       AND settled_at <= $3
       AND ($4::uuid IS NULL OR fleet_id = $4::uuid)
       AND ($5::text IS NULL OR model = $5)
       AND ($6::text IS NULL OR work_type = $6)
     GROUP BY work_type, work_id
)
SELECT work_type, work_id::text, workspace_id, fleet_id, fleet_name, recorded_at,
       charged_nanos, input_tokens, output_tokens, runtime_ms, line_count,
       unpriced_tokens, models, postures
  FROM grouped
 WHERE $7::timestamptz IS NULL
    OR (recorded_at, work_id, work_type) < ($7, $8::uuid, $9::text)
 ORDER BY recorded_at DESC, work_id DESC, work_type DESC
 LIMIT $10

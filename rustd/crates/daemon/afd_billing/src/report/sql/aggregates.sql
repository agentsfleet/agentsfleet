WITH scoped AS (
    SELECT work_type, work_id, fleet_id, fleet_name, model, charged_nanos,
           input_token_count, output_token_count, runtime_ms, unpriced_token_count,
           ((fleet_id = $4 OR $4 IS NULL)
             AND ($5::text IS NULL OR model = $5)
             AND ($6::text IS NULL OR work_type = $6)) AS selected
      FROM billing.usage_lines
     WHERE tenant_id = $1
       AND created_at >= $2 AND created_at <= $3 AND settled_at <= $3
)
SELECT CASE WHEN GROUPING(fleet_id) = 0 THEN $8::int
            WHEN GROUPING(model) = 0 THEN $9::int ELSE $7::int END AS dimension,
       CASE WHEN GROUPING(fleet_id) = 0 THEN fleet_id END AS fleet_key,
       CASE WHEN GROUPING(fleet_id) <> 0 AND GROUPING(model) = 0 THEN model END AS model_key,
       CASE WHEN GROUPING(fleet_id) = 0 THEN MAX(fleet_name) END AS label,
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint AS charged_nanos,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint AS input_tokens,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint AS output_tokens,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint AS runtime_ms,
       COUNT(*) FILTER (WHERE selected)::bigint AS line_count,
       COUNT(DISTINCT (work_type, work_id)) FILTER (WHERE selected)::bigint AS activity_count,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint AS unpriced_tokens
  FROM scoped
 GROUP BY GROUPING SETS ((), (fleet_id), (model))

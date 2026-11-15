-- Full-period aggregates, filter choices and daily buckets over one tenant's
-- settled ledger. Lines are grouped by (work, fleet, model, UTC day) first:
-- every filter and every grouping set keys on those columns, so the sums are
-- unchanged, and the grouping sets then sort a fraction of the rows. At 224k
-- lines in a month that is ~0.4 s against ~1.0 s warm (M249_004 §1 evidence).
WITH lines AS (
    SELECT work_type, work_id, fleet_id, model,
           (created_at AT TIME ZONE 'UTC')::date AS day,
           MAX(fleet_name) AS fleet_name,
           SUM(charged_nanos) AS charged_nanos,
           SUM(input_token_count) AS input_tokens,
           SUM(output_token_count) AS output_tokens,
           SUM(runtime_ms) AS runtime_ms,
           COUNT(*) AS line_count,
           SUM(unpriced_token_count) AS unpriced_tokens
      FROM billing.usage_lines
     WHERE tenant_id = $1
       AND created_at >= $2 AND created_at <= $3 AND settled_at <= $3
     GROUP BY work_type, work_id, fleet_id, model, day
), scoped AS (
    SELECT *,
           ((fleet_id = $4 OR $4 IS NULL)
             AND ($5::text IS NULL OR model = $5)
             AND ($6::text IS NULL OR work_type = $6)) AS selected
      FROM lines
)
SELECT CASE WHEN GROUPING(fleet_id) = 0 THEN $8::int
            WHEN GROUPING(model) = 0 THEN $9::int
            WHEN GROUPING(day) = 0 THEN $10::int ELSE $7::int END AS dimension,
       CASE WHEN GROUPING(fleet_id) = 0 THEN fleet_id END AS fleet_key,
       CASE WHEN GROUPING(fleet_id) <> 0 AND GROUPING(model) = 0 THEN model END AS model_key,
       CASE WHEN GROUPING(fleet_id) = 0 THEN MAX(fleet_name) END AS label,
       CASE WHEN GROUPING(day) = 0 THEN day END AS day,
       CASE WHEN GROUPING(fleet_id) = 0 THEN
           COALESCE(ARRAY_AGG(DISTINCT model ORDER BY model)
               FILTER (WHERE selected AND model <> ''), ARRAY[]::text[])
           ELSE ARRAY[]::text[] END AS models,
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint AS charged_nanos,
       COALESCE(SUM(input_tokens) FILTER (WHERE selected), 0)::bigint AS input_tokens,
       COALESCE(SUM(output_tokens) FILTER (WHERE selected), 0)::bigint AS output_tokens,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint AS runtime_ms,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint AS line_count,
       COUNT(DISTINCT (work_type, work_id)) FILTER (WHERE selected)::bigint AS activity_count,
       COALESCE(SUM(unpriced_tokens) FILTER (WHERE selected), 0)::bigint AS unpriced_tokens
  FROM scoped
 GROUP BY GROUPING SETS ((), (fleet_id), (model), (day))

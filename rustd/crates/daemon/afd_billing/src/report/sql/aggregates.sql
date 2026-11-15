-- Full-period totals, spending groups and filter choices from the daily rollup.
-- A report covers every whole UTC day from the window's first through its last.
-- Groups come from the unfiltered period, so they still name every choice;
-- their sums and counts cover only the selected fleet or setup work.
WITH cells AS (
    SELECT fleet_id, model, day, fleet_name, charged_nanos, input_tokens, output_tokens,
           runtime_ms, line_count, unpriced_tokens,
           ((fleet_id = $4 OR $4::uuid IS NULL) AND ($5::text IS NULL OR work_type = $5)) AS selected
      FROM billing.usage_daily
     WHERE tenant_id = $1
       AND day >= ($2::timestamptz AT TIME ZONE 'UTC')::date
       AND day <= ($3::timestamptz AT TIME ZONE 'UTC')::date
), runs AS (
    SELECT fleet_id, SUM(runs) AS runs
      FROM billing.runs_daily
     WHERE tenant_id = $1
       AND day >= ($2::timestamptz AT TIME ZONE 'UTC')::date
       AND day <= ($3::timestamptz AT TIME ZONE 'UTC')::date
       AND (fleet_id = $4 OR $4::uuid IS NULL) AND ($5::text IS NULL OR work_type = $5)
     GROUP BY fleet_id
)
SELECT $6::int AS dimension, NULL::uuid AS fleet_key, NULL::text AS model_key, NULL::text AS label,
       NULL::date AS day, ARRAY[]::text[] AS models,
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint AS charged_nanos,
       COALESCE(SUM(input_tokens) FILTER (WHERE selected), 0)::bigint AS input_tokens,
       COALESCE(SUM(output_tokens) FILTER (WHERE selected), 0)::bigint AS output_tokens,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint AS runtime_ms,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint AS line_count,
       (SELECT COALESCE(SUM(runs), 0) FROM runs)::bigint AS activity_count,
       COALESCE(SUM(unpriced_tokens) FILTER (WHERE selected), 0)::bigint AS unpriced_tokens
  FROM cells
UNION ALL
SELECT $7::int, c.fleet_id, NULL, MAX(c.fleet_name), NULL,
       COALESCE(ARRAY_AGG(DISTINCT c.model ORDER BY c.model) FILTER (WHERE c.selected AND c.model <> ''),
                ARRAY[]::text[]),
       COALESCE(SUM(c.charged_nanos) FILTER (WHERE c.selected), 0)::bigint,
       COALESCE(SUM(c.input_tokens) FILTER (WHERE c.selected), 0)::bigint,
       COALESCE(SUM(c.output_tokens) FILTER (WHERE c.selected), 0)::bigint,
       COALESCE(SUM(c.runtime_ms) FILTER (WHERE c.selected), 0)::bigint,
       COALESCE(SUM(c.line_count) FILTER (WHERE c.selected), 0)::bigint,
       COALESCE(MAX(r.runs), 0)::bigint,
       COALESCE(SUM(c.unpriced_tokens) FILTER (WHERE c.selected), 0)::bigint
  FROM cells c
  LEFT JOIN runs r ON r.fleet_id IS NOT DISTINCT FROM c.fleet_id
 GROUP BY c.fleet_id
UNION ALL
SELECT $8::int, NULL, model, NULL, NULL, ARRAY[]::text[],
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_tokens) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_tokens) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint,
       NULL::bigint,
       COALESCE(SUM(unpriced_tokens) FILTER (WHERE selected), 0)::bigint
  FROM cells GROUP BY model
UNION ALL
SELECT $9::int, NULL, NULL, NULL, day, ARRAY[]::text[],
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_tokens) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_tokens) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint,
       NULL::bigint,
       COALESCE(SUM(unpriced_tokens) FILTER (WHERE selected), 0)::bigint
  FROM cells GROUP BY day

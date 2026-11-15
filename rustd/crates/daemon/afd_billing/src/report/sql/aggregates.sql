-- Full-period totals, spending groups and filter choices from the daily rollup.
-- A report covers every whole UTC day from the window's first through its last.
-- Groups come from the unfiltered period, so they still name every choice;
-- their sums and counts cover only the selected fleet or setup work.
-- Fleet rows group cells and run counts together by `fleet_id` rather than
-- joining them: grouping matches a NULL fleet (setup) to itself and hashes,
-- where a NULL-safe join could only loop over every pair.
WITH cells AS (
    SELECT fleet_id, model, day, fleet_name, charged_nanos, input_token_count, output_token_count,
           runtime_ms, line_count, unpriced_token_count,
           ((fleet_id = $4 OR $4::uuid IS NULL) AND ($5::text IS NULL OR work_type = $5)) AS selected
      FROM billing.usage_daily
     WHERE tenant_id = $1
       AND day >= ($2::timestamptz AT TIME ZONE 'UTC')::date
       AND day <= ($3::timestamptz AT TIME ZONE 'UTC')::date
), runs AS (
    SELECT fleet_id, run_count
      FROM billing.runs_daily
     WHERE tenant_id = $1
       AND day >= ($2::timestamptz AT TIME ZONE 'UTC')::date
       AND day <= ($3::timestamptz AT TIME ZONE 'UTC')::date
       AND (fleet_id = $4 OR $4::uuid IS NULL) AND ($5::text IS NULL OR work_type = $5)
), fleet_rows AS (
    SELECT fleet_id, fleet_name, model, selected, charged_nanos, input_token_count,
           output_token_count, runtime_ms, line_count, unpriced_token_count, NULL::bigint AS run_count
      FROM cells
    UNION ALL
    SELECT fleet_id, NULL, NULL, false, 0, 0, 0, 0, 0, 0, run_count
      FROM runs
)
SELECT $6::int AS dimension, NULL::uuid AS fleet_key, NULL::text AS model_key, NULL::text AS label,
       NULL::date AS day, ARRAY[]::text[] AS models,
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint AS charged_nanos,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint AS input_tokens,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint AS output_tokens,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint AS runtime_ms,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint AS line_count,
       (SELECT COALESCE(SUM(run_count), 0) FROM runs)::bigint AS activity_count,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint AS unpriced_tokens
  FROM cells
UNION ALL
SELECT $7::int, fleet_id, NULL, MAX(fleet_name), NULL,
       COALESCE(ARRAY_AGG(DISTINCT model ORDER BY model) FILTER (WHERE selected AND model <> ''),
                ARRAY[]::text[]),
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(run_count), 0)::bigint,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint
  FROM fleet_rows
 GROUP BY fleet_id
HAVING bool_or(model IS NOT NULL)
UNION ALL
SELECT $8::int, NULL, model, NULL, NULL, ARRAY[]::text[],
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint,
       NULL::bigint,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint
  FROM cells GROUP BY model
UNION ALL
SELECT $9::int, NULL, NULL, NULL, day, ARRAY[]::text[],
       COALESCE(SUM(charged_nanos) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(input_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(output_token_count) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(runtime_ms) FILTER (WHERE selected), 0)::bigint,
       COALESCE(SUM(line_count) FILTER (WHERE selected), 0)::bigint,
       NULL::bigint,
       COALESCE(SUM(unpriced_token_count) FILTER (WHERE selected), 0)::bigint
  FROM cells GROUP BY day

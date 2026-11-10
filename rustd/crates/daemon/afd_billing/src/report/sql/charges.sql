-- One activity's charges in the order they happened: by `(requested_at, id)`,
-- which is unique, so a page resumes exactly after its last row. `seq` is
-- unique only within a line's `kind` (M245), so it no longer orders a page.
SELECT id, kind, seq AS sequence, model, posture, charged_nanos,
       input_token_count AS input_tokens, output_token_count AS output_tokens,
       runtime_ms, unpriced_token_count AS unpriced_tokens, requested_at, settled_at
  FROM billing.usage_lines
 WHERE tenant_id = $1 AND work_type = $2 AND work_id = $3
   AND created_at >= $4 AND created_at <= $5 AND settled_at <= $5
   AND ($6::text IS NULL OR model = $6)
   AND ($7::timestamptz IS NULL OR (requested_at, id) > ($7::timestamptz, $11::uuid))
   AND (fleet_id = $9 OR $9 IS NULL)
   AND ($10::text IS NULL OR work_type = $10)
 ORDER BY requested_at, id
 LIMIT $8

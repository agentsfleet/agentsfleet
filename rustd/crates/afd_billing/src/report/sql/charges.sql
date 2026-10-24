SELECT id::text, seq AS sequence, model, posture, charged_nanos,
       input_token_count AS input_tokens, output_token_count AS output_tokens,
       runtime_ms, unpriced_token_count AS unpriced_tokens, requested_at, settled_at
  FROM billing.usage_lines
 WHERE tenant_id = $1::uuid AND work_type = $2 AND work_id = $3::uuid
   AND created_at >= $4 AND created_at <= $5 AND settled_at <= $5
   AND ($6::text IS NULL OR model = $6)
   AND ($7::bigint IS NULL OR seq > $7)
   AND ($9::uuid IS NULL OR fleet_id = $9::uuid)
   AND ($10::text IS NULL OR work_type = $10)
 ORDER BY seq
 LIMIT $8

UPDATE billing.usage_lines
SET settled_at = now(), updated_at = now()
WHERE work_type = $1 AND work_id = $2 AND settled_at IS NULL
  AND (kind, seq) IN (SELECT * FROM unnest($3::text[], $4::bigint[]))
RETURNING tenant_id, fleet_id, fleet_name, model, kind, seq, created_at, charged_nanos,
          input_token_count, output_token_count, runtime_ms, unpriced_token_count

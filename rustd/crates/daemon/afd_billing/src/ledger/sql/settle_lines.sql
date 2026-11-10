UPDATE billing.usage_lines
SET settled_at = now(), updated_at = now()
WHERE work_type = $1 AND work_id = $2 AND settled_at IS NULL
  AND (kind, seq) IN (SELECT * FROM unnest($3::text[], $4::bigint[]))
RETURNING charged_nanos

UPDATE billing.usage_lines
SET settled_at = now(), updated_at = now()
WHERE work_type = $1 AND work_id = $2::uuid AND settled_at IS NULL
  AND ($3::bigint[] IS NULL OR seq = ANY($3::bigint[]))
RETURNING charged_nanos

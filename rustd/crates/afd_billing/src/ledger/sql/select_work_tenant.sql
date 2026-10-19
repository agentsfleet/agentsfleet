SELECT tenant_id::text AS tenant_id FROM billing.holds WHERE work_type = $1 AND work_id = $2::uuid
UNION
SELECT tenant_id::text FROM billing.usage_lines
WHERE work_type = $1 AND work_id = $2::uuid AND settled_at IS NULL
LIMIT 1

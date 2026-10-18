SELECT COALESCE(SUM(charged_nanos), 0)::bigint
FROM billing.usage_lines
WHERE work_type = $1 AND work_id = $2::uuid

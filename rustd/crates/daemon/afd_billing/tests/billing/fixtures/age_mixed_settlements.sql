UPDATE billing.usage_lines
SET created_at = now() - $2::interval
    - CASE WHEN work_id = $3 AND seq = $4
        THEN interval '1 millisecond' ELSE interval '0 milliseconds' END
WHERE tenant_id = $1

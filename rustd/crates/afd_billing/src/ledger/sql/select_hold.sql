SELECT ceiling_nanos, captured_nanos, released_at IS NOT NULL AS released
FROM billing.holds
WHERE work_type = $1 AND work_id = $2::uuid

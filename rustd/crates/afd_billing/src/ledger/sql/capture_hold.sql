UPDATE billing.holds
SET captured_nanos = captured_nanos + $3, updated_at = now()
WHERE work_type = $1 AND work_id = $2::uuid

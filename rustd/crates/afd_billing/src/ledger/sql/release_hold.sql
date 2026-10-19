UPDATE billing.holds
SET released_at = now(), updated_at = now()
WHERE work_type = $1 AND work_id = $2::uuid AND released_at IS NULL

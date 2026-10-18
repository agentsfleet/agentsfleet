UPDATE runner.runners
SET degraded_reason = $2::text, updated_at = now()
WHERE id = $1::uuid AND degraded_reason IS DISTINCT FROM $2::text

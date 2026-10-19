UPDATE runner.runners
SET capability_report = $2::jsonb, degraded_reason = $3::text, updated_at = now()
WHERE id = $1::uuid

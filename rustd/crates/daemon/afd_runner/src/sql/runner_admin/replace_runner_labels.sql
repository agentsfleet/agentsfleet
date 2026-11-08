UPDATE runner.runners
SET labels = $2::jsonb,
    updated_at = CASE WHEN labels IS DISTINCT FROM $2::jsonb THEN now() ELSE updated_at END
WHERE id = $1::uuid

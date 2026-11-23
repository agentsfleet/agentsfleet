UPDATE runner.runners
SET run_version = run_version + 1, last_run_type = $3::text, updated_at = now()
WHERE id = $1 AND run_version = $2::bigint

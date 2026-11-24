UPDATE runner.runners
SET run_version = run_version + 1, updated_at = now()
WHERE id = $1 AND run_version = $2::bigint

UPDATE runner.leases SET task_id = $1::uuid, updated_at = now()
WHERE id = $2::uuid AND runner_id = $3::uuid AND task_id IS NULL

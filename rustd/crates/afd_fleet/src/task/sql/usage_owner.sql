SELECT task_id::text, fleet_id::text, workspace_id::text FROM runner.leases
WHERE id = $1::uuid AND task_id IS NOT NULL

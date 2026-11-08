SELECT fleet_id::text, workspace_id::text
FROM runner.leases
WHERE id = $1::uuid AND runner_id = $2::uuid

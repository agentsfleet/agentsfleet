SELECT fleet_id, workspace_id
FROM thread.runs
WHERE id = $1 AND runner_id = $2 AND fleet_id IS NOT NULL

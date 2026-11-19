SELECT task_id, fleet_id, workspace_id FROM runner.runs
WHERE id = $1 AND task_id IS NOT NULL

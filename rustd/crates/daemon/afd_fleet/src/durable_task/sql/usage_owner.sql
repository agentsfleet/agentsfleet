SELECT durable_task_id, fleet_id, workspace_id FROM thread.runs
WHERE id = $1 AND durable_task_id IS NOT NULL

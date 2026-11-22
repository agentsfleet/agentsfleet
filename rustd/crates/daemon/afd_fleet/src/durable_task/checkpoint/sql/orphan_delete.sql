DELETE FROM fleet.durable_task_objects
WHERE workspace_id = $1 AND id = ANY($2) AND tombstoned_at IS NOT NULL

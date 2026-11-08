SELECT id FROM fleet.task_objects
WHERE workspace_id = $1 AND id = ANY($2) ORDER BY id FOR UPDATE

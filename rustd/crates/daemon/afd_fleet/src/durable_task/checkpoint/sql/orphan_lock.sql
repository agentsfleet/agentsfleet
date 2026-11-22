SELECT id FROM runner.checkpoint_objects
WHERE workspace_id = $1 AND id = ANY($2) ORDER BY id FOR UPDATE

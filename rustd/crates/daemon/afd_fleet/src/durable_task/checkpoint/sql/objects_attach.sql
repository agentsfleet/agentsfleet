INSERT INTO runner.checkpoint_object_refs (checkpoint_object_id, workspace_id, checkpoint_id, checkpoint_base_id, created_at)
SELECT o.id, o.workspace_id, $3, $4, now()
FROM runner.checkpoint_objects o
WHERE o.id = ANY($1) AND o.workspace_id = $2 AND o.tombstoned_at IS NULL
ON CONFLICT DO NOTHING

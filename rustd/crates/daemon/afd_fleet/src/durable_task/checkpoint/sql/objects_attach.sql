INSERT INTO fleet.durable_task_object_refs (durable_task_object_id, workspace_id, durable_task_checkpoint_id, durable_task_base_id, created_at)
SELECT o.id, o.workspace_id, $3, $4, now()
FROM fleet.durable_task_objects o
WHERE o.id = ANY($1) AND o.workspace_id = $2 AND o.tombstoned_at IS NULL
ON CONFLICT DO NOTHING

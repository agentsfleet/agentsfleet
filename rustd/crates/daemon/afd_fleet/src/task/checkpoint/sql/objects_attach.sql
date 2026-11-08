INSERT INTO fleet.task_object_refs (task_object_id, workspace_id, task_checkpoint_id, task_base_id, created_at)
SELECT o.id, o.workspace_id, $3::uuid, $4::uuid, now()
FROM fleet.task_objects o
WHERE o.id = ANY($1::text[]::uuid[]) AND o.workspace_id = $2::uuid AND o.tombstoned_at IS NULL
ON CONFLICT DO NOTHING

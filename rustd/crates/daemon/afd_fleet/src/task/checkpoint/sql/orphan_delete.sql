DELETE FROM fleet.task_objects
WHERE workspace_id = $1::uuid AND id = ANY($2::text[]::uuid[]) AND tombstoned_at IS NOT NULL

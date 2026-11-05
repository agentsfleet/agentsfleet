UPDATE fleet.task_objects o SET tombstoned_at = clock_timestamp(), updated_at = now()
WHERE o.workspace_id = $1::uuid AND o.id = ANY($2::text[]::uuid[]) AND o.tombstoned_at IS NULL
  AND o.reclaim_after_at <= statement_timestamp()
  AND NOT EXISTS (SELECT 1 FROM fleet.task_object_refs r WHERE r.task_object_id = o.id)
RETURNING o.id::text

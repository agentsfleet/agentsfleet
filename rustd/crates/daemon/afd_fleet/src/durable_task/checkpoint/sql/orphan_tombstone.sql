UPDATE fleet.durable_task_objects o SET tombstoned_at = clock_timestamp(), updated_at = now()
WHERE o.workspace_id = $1 AND o.id = ANY($2) AND o.tombstoned_at IS NULL
  AND o.reclaim_after_at <= statement_timestamp()
  AND NOT EXISTS (SELECT 1 FROM fleet.durable_task_object_refs r WHERE r.durable_task_object_id = o.id)
  AND NOT EXISTS (SELECT 1 FROM runner.turn_checkpoint_refs r WHERE r.durable_task_object_id = o.id)
RETURNING o.id

SELECT NOT EXISTS (
    SELECT 1 FROM fleet.durable_task_object_refs r
    JOIN fleet.durable_task_objects o ON o.id = r.durable_task_object_id AND o.workspace_id = r.workspace_id
    WHERE r.workspace_id = $4 AND
      ((NOT $3 AND r.durable_task_checkpoint_id = $1) OR ($3 AND r.durable_task_base_id = $2))
      AND (o.verified_at IS NULL OR o.tombstoned_at IS NOT NULL)
)

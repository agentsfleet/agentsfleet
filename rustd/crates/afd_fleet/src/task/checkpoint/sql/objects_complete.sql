SELECT NOT EXISTS (
    SELECT 1 FROM fleet.task_object_refs r
    JOIN fleet.task_objects o ON o.id = r.object_id AND o.workspace_id = r.workspace_id
    WHERE r.workspace_id = $4::uuid AND
      ((NOT $3 AND r.checkpoint_id = $1::uuid) OR ($3 AND r.base_id = $2::uuid))
      AND (o.verified_at IS NULL OR o.tombstoned_at IS NOT NULL)
)

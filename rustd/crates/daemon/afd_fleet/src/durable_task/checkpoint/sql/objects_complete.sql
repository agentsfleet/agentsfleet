SELECT NOT EXISTS (
    SELECT 1 FROM runner.checkpoint_object_refs r
    JOIN runner.checkpoint_objects o ON o.id = r.checkpoint_object_id AND o.workspace_id = r.workspace_id
    WHERE r.workspace_id = $4 AND
      ((NOT $3 AND r.checkpoint_id = $1) OR ($3 AND r.checkpoint_base_id = $2))
      AND (o.verified_at IS NULL OR o.tombstoned_at IS NOT NULL)
)

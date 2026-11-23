SELECT o.id, o.workspace_id FROM runner.checkpoint_objects o
WHERE o.tombstoned_at IS NULL AND o.updated_at <= statement_timestamp() - make_interval(secs => $1)
  AND o.reclaim_after_at <= statement_timestamp()
  AND NOT EXISTS (SELECT 1 FROM runner.checkpoint_object_refs r WHERE r.checkpoint_object_id = o.id)
ORDER BY o.updated_at, o.id LIMIT $2

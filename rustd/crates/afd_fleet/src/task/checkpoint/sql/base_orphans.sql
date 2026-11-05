SELECT b.id::text, b.workspace_id::text FROM fleet.task_bases b
WHERE b.updated_at <= statement_timestamp() - make_interval(secs => $1)
  AND NOT EXISTS (SELECT 1 FROM fleet.task_checkpoints c WHERE c.base_id = b.id)
ORDER BY b.updated_at, b.id LIMIT $2

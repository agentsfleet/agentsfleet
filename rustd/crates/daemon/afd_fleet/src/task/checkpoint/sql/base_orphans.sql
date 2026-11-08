SELECT b.id, b.workspace_id FROM fleet.task_bases b
WHERE b.updated_at <= statement_timestamp() - make_interval(secs => $1)
  AND NOT EXISTS (SELECT 1 FROM fleet.task_checkpoints c WHERE c.task_base_id = b.id)
ORDER BY b.updated_at, b.id LIMIT $2

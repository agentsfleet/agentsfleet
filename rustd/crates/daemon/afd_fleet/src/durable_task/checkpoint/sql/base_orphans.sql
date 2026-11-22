SELECT b.id, b.workspace_id FROM runner.checkpoint_bases b
WHERE b.updated_at <= statement_timestamp() - make_interval(secs => $1)
  AND NOT EXISTS (SELECT 1 FROM runner.checkpoints c WHERE c.checkpoint_base_id = b.id)
ORDER BY b.updated_at, b.id LIMIT $2

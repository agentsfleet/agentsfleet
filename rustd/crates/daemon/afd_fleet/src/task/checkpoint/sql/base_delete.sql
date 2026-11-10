DELETE FROM fleet.task_bases b
WHERE b.id = $1 AND b.workspace_id = $2 AND b.updated_at <= statement_timestamp() - make_interval(secs => $3)
  AND NOT EXISTS (SELECT 1 FROM fleet.task_checkpoints c WHERE c.task_base_id = b.id)

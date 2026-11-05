UPDATE fleet.task_bases b SET retired_at = clock_timestamp(), updated_at = now()
WHERE b.workspace_id = $1::uuid AND b.repository = $2 AND b.ready_at IS NULL AND b.retired_at IS NULL
  AND NOT EXISTS (SELECT 1 FROM fleet.task_checkpoints c WHERE c.base_id = b.id AND c.abandoned_at IS NULL)

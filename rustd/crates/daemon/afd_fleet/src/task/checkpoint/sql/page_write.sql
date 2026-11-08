UPDATE fleet.task_checkpoint_pages SET digest = $2, body = $3, updated_at = now() WHERE id = $1 AND body IS NULL

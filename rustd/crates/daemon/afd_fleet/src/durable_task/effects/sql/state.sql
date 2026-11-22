UPDATE fleet.durable_task_effects SET status = $2, updated_at = now()
WHERE id = $1 AND status = $3

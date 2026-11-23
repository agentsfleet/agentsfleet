UPDATE fleet.durable_task_effects SET status = $3, commit_oid = $2, updated_at = now()
WHERE id = $1 AND status IN ($4, $5)

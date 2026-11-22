UPDATE fleet.durable_task_effect_states SET status = $3, commit_oid = $2, updated_at = now()
WHERE durable_task_effect_id = $1 AND status IN ($4, $5)

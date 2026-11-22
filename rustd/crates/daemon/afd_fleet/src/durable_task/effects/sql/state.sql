UPDATE fleet.durable_task_effect_states SET status = $2, updated_at = now()
WHERE durable_task_effect_id = $1 AND status = $3

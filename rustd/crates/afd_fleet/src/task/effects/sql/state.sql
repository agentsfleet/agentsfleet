UPDATE fleet.task_effect_states SET status = $2, updated_at = now()
WHERE task_effect_id = $1::uuid AND status = $3

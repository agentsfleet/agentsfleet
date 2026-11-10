UPDATE fleet.task_effect_states SET status = $3, commit_oid = $2,
    confirmed_at = clock_timestamp(), updated_at = now()
WHERE task_effect_id = $1 AND status IN ($4, $5)

SELECT NOT EXISTS (
    SELECT 1 FROM fleet.task_effects e JOIN fleet.task_effect_states s ON s.task_effect_id = e.id
    WHERE e.task_id = $1 AND s.status IN ($2, $3, $4)
)

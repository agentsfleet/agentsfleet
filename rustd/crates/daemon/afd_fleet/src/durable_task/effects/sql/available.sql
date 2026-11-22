SELECT NOT EXISTS (
    SELECT 1 FROM fleet.durable_task_effects e JOIN fleet.durable_task_effect_states s ON s.durable_task_effect_id = e.id
    WHERE e.durable_task_id = $1 AND s.status IN ($2, $3, $4)
)

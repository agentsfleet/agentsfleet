SELECT NOT EXISTS (
    SELECT 1 FROM fleet.durable_task_effects e
    WHERE e.durable_task_id = $1 AND e.status IN ($2, $3, $4)
)

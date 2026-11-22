UPDATE fleet.durable_task_inputs SET answered_at = COALESCE(answered_at, clock_timestamp()), updated_at = clock_timestamp()
WHERE durable_task_item_id = $1

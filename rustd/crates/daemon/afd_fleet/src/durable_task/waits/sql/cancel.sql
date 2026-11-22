UPDATE fleet.durable_task_waits SET cancelled_at = clock_timestamp(), updated_at = now()
WHERE durable_task_id = $1 AND consumed_at IS NULL AND cancelled_at IS NULL

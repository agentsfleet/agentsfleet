UPDATE fleet.task_waits SET cancelled_at = clock_timestamp(), updated_at = now()
WHERE task_id = $1 AND consumed_at IS NULL AND cancelled_at IS NULL

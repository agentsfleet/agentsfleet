UPDATE fleet.durable_tasks SET wait_cancelled_at = clock_timestamp(), wait_updated_at = now()
WHERE id = $1 AND wait_sequence IS NOT NULL AND wait_cancelled_at IS NULL

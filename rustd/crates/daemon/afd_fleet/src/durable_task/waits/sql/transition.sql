UPDATE fleet.durable_tasks SET status = $2, generation = generation + 1,
    stop_generation = stop_generation + 1, resource_version = resource_version + 1,
    terminal_at = CASE WHEN $3 THEN clock_timestamp() ELSE terminal_at END,
    checkpoint_cleanup_at = clock_timestamp(), updated_at = now()
WHERE id = $1

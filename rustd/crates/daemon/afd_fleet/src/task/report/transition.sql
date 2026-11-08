UPDATE fleet.tasks SET status = $2, status_reason = $3,
    generation = generation + 1, stop_generation = stop_generation + 1, resource_version = resource_version + 1,
    terminal_at = CASE WHEN $4 THEN clock_timestamp() ELSE terminal_at END,
    last_transition_at = clock_timestamp(), checkpoint_cleanup_at = clock_timestamp(), updated_at = now()
WHERE id = $1

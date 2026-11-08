UPDATE fleet.tasks t SET status = $2,
    status_reason = CASE WHEN expires_at <= clock_timestamp() THEN $3 ELSE $4 END,
    generation = generation + 1, stop_generation = stop_generation + 1, resource_version = resource_version + 1,
    terminal_at = clock_timestamp(), checkpoint_cleanup_at = clock_timestamp(), last_transition_at = clock_timestamp(), updated_at = now()
WHERE id = $1::uuid AND terminal_at IS NULL AND (expires_at <= clock_timestamp()
    OR used_leases >= max_leases AND NOT EXISTS (SELECT 1 FROM runner.leases l WHERE l.task_id = t.id
        AND l.status = $5 AND l.lease_expires_at > clock_timestamp()))
RETURNING status_reason

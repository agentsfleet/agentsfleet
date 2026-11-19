UPDATE thread.threads
SET run_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
    last_runner_id = CASE WHEN hold_expires_at > clock_timestamp() THEN last_runner_id END
WHERE id = $1 AND fencing_token = $2

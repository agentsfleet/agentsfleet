-- The parent fleet, current thread and run are already locked. Re-read this
-- clock after any competing reservation waited for the counter row.
SELECT EXISTS (
    SELECT 1 FROM thread.threads
    WHERE fleet_id = $1 AND last_runner_id = $2 AND fencing_token = $3
      AND run_expires_at > clock_timestamp()
)

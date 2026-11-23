UPDATE thread.turns SET interpreted_at = clock_timestamp(), updated_at = clock_timestamp()
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3 AND interpreted_at IS NULL

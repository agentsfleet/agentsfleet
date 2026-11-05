UPDATE fleet.task_completion_fanout SET next_attempt_at = clock_timestamp() + $2::interval, updated_at = now()
WHERE completion_id = $1::uuid AND finished_at IS NULL

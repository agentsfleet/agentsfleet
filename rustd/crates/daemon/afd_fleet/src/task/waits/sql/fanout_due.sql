SELECT task_completion_id FROM fleet.task_completion_fanout
WHERE finished_at IS NULL AND next_attempt_at <= statement_timestamp()
ORDER BY next_attempt_at, task_completion_id LIMIT $1

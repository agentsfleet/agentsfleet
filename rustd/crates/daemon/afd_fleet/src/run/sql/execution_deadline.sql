SELECT LEAST(created_at + LEAST($2::interval, make_interval(secs => max_runtime_seconds)), task_execution_deadline_at) AS execution_deadline
FROM runner.runs
WHERE id = $1

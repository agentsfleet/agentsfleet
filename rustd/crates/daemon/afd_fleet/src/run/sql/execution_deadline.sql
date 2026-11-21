SELECT task_execution_deadline_at AS execution_deadline
FROM thread.runs
WHERE id = $1

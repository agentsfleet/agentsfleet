SELECT LEAST(created_at + $2::interval, task_execution_deadline_at) AS execution_deadline
FROM runner.leases
WHERE id = $1

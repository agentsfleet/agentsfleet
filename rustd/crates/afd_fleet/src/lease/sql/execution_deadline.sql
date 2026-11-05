SELECT LEAST(created_at + $2::interval, task_execution_deadline) AS execution_deadline
FROM runner.leases
WHERE id = $1::uuid

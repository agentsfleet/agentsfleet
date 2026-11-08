INSERT INTO fleet.task_adoptions (operation_id, task_id, fleet_id, workspace_id, request_digest, created_at)
VALUES ($1, $2, $3, $4, $5, clock_timestamp())

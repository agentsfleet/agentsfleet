INSERT INTO fleet.task_adoptions (operation_id, task_id, fleet_id, workspace_id, request_digest, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, clock_timestamp())

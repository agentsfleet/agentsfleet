SELECT request_digest FROM fleet.task_adoptions
WHERE task_id = $1::uuid AND operation_id = $2::uuid AND fleet_id = $3::uuid AND workspace_id = $4::uuid

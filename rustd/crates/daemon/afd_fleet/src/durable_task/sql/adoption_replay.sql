SELECT request_digest FROM fleet.durable_task_adoptions
WHERE durable_task_id = $1 AND operation_id = $2 AND fleet_id = $3 AND workspace_id = $4

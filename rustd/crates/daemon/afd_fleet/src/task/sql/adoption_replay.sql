SELECT request_digest FROM fleet.task_adoptions
WHERE task_id = $1 AND operation_id = $2 AND fleet_id = $3 AND workspace_id = $4

SELECT generation FROM fleet.tasks
WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
FOR UPDATE

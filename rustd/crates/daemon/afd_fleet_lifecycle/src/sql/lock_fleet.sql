SELECT status, resource_version FROM fleet.fleets
WHERE id = $1 AND workspace_id = $2
FOR UPDATE

SELECT status FROM fleet.fleets
WHERE id = $1 AND workspace_id = $2

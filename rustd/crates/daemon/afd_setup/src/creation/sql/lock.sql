SELECT id FROM fleet.fleets WHERE id = $1 AND workspace_id = $2 FOR UPDATE

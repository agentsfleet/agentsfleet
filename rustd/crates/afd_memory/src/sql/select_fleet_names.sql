SELECT id::text, name FROM fleet.fleets WHERE workspace_id = $1::uuid

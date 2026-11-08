SELECT id::text FROM fleet.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid FOR UPDATE

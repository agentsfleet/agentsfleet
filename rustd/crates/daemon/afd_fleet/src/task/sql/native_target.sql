SELECT target FROM fleet.tasks WHERE id = $1::uuid AND fleet_id = $2::uuid AND workspace_id = $3::uuid

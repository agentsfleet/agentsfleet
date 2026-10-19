SELECT revision FROM fleet.revisions WHERE id = $1::uuid AND fleet_id = $2::uuid

UPDATE fleet.fleets SET status = $1, updated_at = now() WHERE id = $2::uuid

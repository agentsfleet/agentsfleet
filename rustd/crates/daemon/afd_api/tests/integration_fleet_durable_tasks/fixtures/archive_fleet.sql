UPDATE fleet.fleets SET status = $2, resource_version = resource_version + 1
WHERE id = $1

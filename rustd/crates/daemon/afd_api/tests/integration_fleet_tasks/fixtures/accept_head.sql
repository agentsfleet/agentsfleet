UPDATE fleet.tasks t SET accepted_revision_id = f.head_revision_id
FROM fleet.fleets f
WHERE f.id = $1 AND t.fleet_id = f.id

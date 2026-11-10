SELECT r.definition::text
FROM fleet.fleets f
JOIN fleet.revisions r ON r.id = COALESCE($2, f.head_revision_id) AND r.fleet_id = f.id
WHERE f.id = $1

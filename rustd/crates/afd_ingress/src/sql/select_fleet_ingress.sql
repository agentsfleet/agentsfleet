SELECT f.workspace_id::text AS workspace_id,
       f.status,
       (r.definition->'config')::text AS document
FROM fleet.fleets f
JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
WHERE f.id = $1::uuid

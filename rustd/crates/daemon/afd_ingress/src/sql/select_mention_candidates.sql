SELECT f.id::text AS fleet_id,
       f.status,
       (r.definition->'config')::text AS document
FROM fleet.fleets f
JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
WHERE f.workspace_id = $1::uuid
  AND f.status = ANY($2::text[])
ORDER BY f.id

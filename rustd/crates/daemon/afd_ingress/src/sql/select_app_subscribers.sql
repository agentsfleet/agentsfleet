SELECT f.id AS fleet_id,
       f.workspace_id AS workspace_id,
       f.status,
       (r.definition->'config')::text AS document
FROM fleet.fleets f
JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
JOIN connector.integration_grants g ON g.fleet_id = f.id
WHERE f.workspace_id = $1
  AND f.status = $2
  AND g.service = $3
  AND g.status = $4
ORDER BY f.id

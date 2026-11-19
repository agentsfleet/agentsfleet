-- The fleets of a workspace a mention could reach, with the saved name a
-- person addresses each by and the head setup routing reads.
SELECT f.id AS fleet_id,
       f.name,
       f.status,
       (r.definition->'config')::text AS document
FROM fleet.fleets f
JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
WHERE f.workspace_id = $1
  AND f.status = ANY($2::text[])
ORDER BY f.id

SELECT f.id::text AS fleet_id,
       f.status,
       COALESCE(r.definition->'config' = $3::jsonb, false) AS is_resident
FROM fleet.fleets f
LEFT JOIN fleet.revisions r ON r.id = f.head_revision_id AND r.fleet_id = f.id
WHERE f.workspace_id = $1::uuid AND f.name = $2

WITH workspace AS MATERIALIZED (
    SELECT id FROM identity.lock_workspace($2::uuid)
)
SELECT f.id::text FROM fleet.fleets f
JOIN workspace w ON w.id = f.workspace_id
WHERE f.id = $1::uuid
FOR KEY SHARE OF f

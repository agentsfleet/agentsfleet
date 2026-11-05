WITH workspace AS MATERIALIZED (
    SELECT id FROM identity.workspaces WHERE id = $2::uuid FOR KEY SHARE
)
SELECT f.id::text FROM fleet.fleets f
JOIN workspace w ON w.id = f.workspace_id
WHERE f.id = $1::uuid
FOR KEY SHARE OF f

SELECT id, name, created_at, tenant_id
FROM identity.workspaces
WHERE tenant_id = ANY($1)
  AND name = $2
  AND (created_at, id) > ($3::timestamptz, $4)
ORDER BY created_at ASC, id ASC
LIMIT $5

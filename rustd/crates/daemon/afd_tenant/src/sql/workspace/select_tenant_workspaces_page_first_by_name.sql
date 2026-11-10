SELECT id, name, created_at, tenant_id
FROM identity.workspaces
WHERE tenant_id = ANY($1)
  AND name = $2
ORDER BY created_at ASC, id ASC
LIMIT $3

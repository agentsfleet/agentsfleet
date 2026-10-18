SELECT id::text, name, created_at, tenant_id::text
FROM identity.workspaces
WHERE tenant_id = ANY($1::uuid[])
  AND name = $2
ORDER BY created_at ASC, id ASC
LIMIT $3

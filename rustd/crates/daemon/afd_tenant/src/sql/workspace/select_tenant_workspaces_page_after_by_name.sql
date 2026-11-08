SELECT id::text, name, created_at, tenant_id::text
FROM identity.workspaces
WHERE tenant_id = ANY($1::uuid[])
  AND name = $2
  AND (created_at, id) > ($3::timestamptz, $4::uuid)
ORDER BY created_at ASC, id ASC
LIMIT $5

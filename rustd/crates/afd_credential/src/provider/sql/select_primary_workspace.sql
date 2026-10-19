SELECT id::text
  FROM identity.workspaces
 WHERE tenant_id = $1::uuid
 ORDER BY created_at ASC, id ASC
 LIMIT 1

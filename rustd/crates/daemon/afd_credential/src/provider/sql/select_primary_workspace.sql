SELECT id
  FROM identity.workspaces
 WHERE tenant_id = $1
 ORDER BY created_at ASC, id ASC
 LIMIT 1

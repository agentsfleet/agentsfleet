SELECT 1
  FROM vault.secrets s
  JOIN (SELECT id FROM identity.workspaces
         WHERE tenant_id = $1::uuid
         ORDER BY created_at ASC, id ASC
         LIMIT 1) w ON s.workspace_id = w.id
 WHERE s.key_name = $2
   FOR UPDATE OF s

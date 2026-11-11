SELECT 1
  FROM vault.secrets s
  JOIN identity.workspaces w ON w.id = s.workspace_id AND w.tenant_id = $1
 WHERE s.key_name = $2
   AND s.workspace_id = $3
   FOR UPDATE OF s

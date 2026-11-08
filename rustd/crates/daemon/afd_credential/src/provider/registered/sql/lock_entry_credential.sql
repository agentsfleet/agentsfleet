SELECT s.id
  FROM model.tenant_models e
  JOIN LATERAL (
        SELECT id FROM identity.workspaces
         WHERE tenant_id = e.tenant_id
         ORDER BY created_at, id LIMIT 1
       ) w ON true
  JOIN vault.secrets s ON s.workspace_id = w.id AND s.key_name = e.secret_ref
 WHERE e.tenant_id = $1::uuid AND e.id::text = $2
   FOR SHARE OF s

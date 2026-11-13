SELECT s.id
  FROM model.tenant_models e
  JOIN vault.secrets s ON s.workspace_id = e.workspace_id AND s.key_name = e.secret_ref
 WHERE e.tenant_id = $1 AND e.id = $2
   FOR SHARE OF s

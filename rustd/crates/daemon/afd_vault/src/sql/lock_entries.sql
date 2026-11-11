SELECT id FROM model.tenant_models
 WHERE tenant_id = $1 AND secret_ref = $2 AND workspace_id = $3
 ORDER BY id
   FOR UPDATE

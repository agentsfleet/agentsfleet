SELECT id, model_name, secret_ref, workspace_id, created_at
  FROM model.tenant_models
 WHERE tenant_id = $1
 ORDER BY created_at DESC, id DESC
 LIMIT $2

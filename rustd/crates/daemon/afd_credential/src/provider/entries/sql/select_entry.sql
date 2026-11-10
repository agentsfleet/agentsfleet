SELECT id, model_name, secret_ref, created_at
  FROM model.tenant_models
 WHERE id = $1 AND tenant_id = $2

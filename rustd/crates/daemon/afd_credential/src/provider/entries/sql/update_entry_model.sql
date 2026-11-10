UPDATE model.tenant_models
   SET model_name = $3, updated_at = now()
 WHERE id = $1 AND tenant_id = $2
RETURNING id, model_name, secret_ref, created_at

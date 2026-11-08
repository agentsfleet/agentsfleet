UPDATE model.tenant_models
   SET model_name = $3, updated_at = now()
 WHERE id = $1::uuid AND tenant_id = $2::uuid
RETURNING id::text AS id, model_name, secret_ref, created_at

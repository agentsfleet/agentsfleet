INSERT INTO model.tenant_models (id, tenant_id, model_name, secret_ref, created_at, updated_at)
VALUES ($1::uuid, $2::uuid, $3, $4, now(), now())
ON CONFLICT (tenant_id, model_name, secret_ref) DO NOTHING
RETURNING id::text AS id, model_name, secret_ref, created_at

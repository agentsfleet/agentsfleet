INSERT INTO model.tenant_models (id, tenant_id, model_name, secret_ref, workspace_id, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, now(), now())
ON CONFLICT (tenant_id, workspace_id, model_name, secret_ref) DO NOTHING

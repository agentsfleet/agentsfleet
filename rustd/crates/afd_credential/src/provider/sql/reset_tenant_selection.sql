INSERT INTO model.tenant_selections (tenant_id, tenant_model_id, created_at, updated_at)
VALUES ($1::uuid, NULL, now(), now())
ON CONFLICT (tenant_id) DO UPDATE SET
    tenant_model_id = NULL,
    updated_at = now()

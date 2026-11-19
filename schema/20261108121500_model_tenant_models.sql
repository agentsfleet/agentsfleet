-- `secret_ref` is the vault `key_name`, never the secret. NULL `workspace_id` means the source
-- workspace was removed; the model row is kept for history.
CREATE TABLE model.tenant_models (
    id          UUID PRIMARY KEY,
    tenant_id   UUID   NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    workspace_id UUID,
    model_name  TEXT   NOT NULL,
    secret_ref  TEXT   NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    updated_at  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_tenant_models_workspace
        FOREIGN KEY (workspace_id, tenant_id)
        REFERENCES identity.workspaces (id, tenant_id)
        ON DELETE SET NULL (workspace_id),
    CONSTRAINT uq_tenant_models_tenant_workspace_model_secret
        UNIQUE (tenant_id, workspace_id, model_name, secret_ref),
    CONSTRAINT uq_tenant_models_id_tenant_id UNIQUE (id, tenant_id)
);

CREATE INDEX idx_tenant_models_tenant_id_created_at
    ON model.tenant_models (tenant_id, created_at DESC);

CREATE INDEX idx_tenant_models_workspace_id
    ON model.tenant_models (workspace_id) WHERE workspace_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON model.tenant_models TO api_runtime;

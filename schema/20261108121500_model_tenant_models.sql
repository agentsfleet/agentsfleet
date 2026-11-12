-- A tenant's own models, each pointing at the vault key that calls it. A table,
-- not a selection column, because a tenant keeps several and picks one as default.
-- `secret_ref` is the vault `key_name`, never the secret (VLT).
-- Workspace removal clears the source reference while retaining model history.
--
-- `UNIQUE (id, tenant_id)` exists for the composite references from
-- `model.tenant_selections` and `authoring.drafts`, which must prove the model
-- belongs to the same tenant.
CREATE TABLE model.tenant_models (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_tenant_models_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id   UUID   NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    -- Reader: afd_credential/src/provider/registered/sql/entry.sql resolves this exact vault scope.
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

-- The tenant's model list, newest first.
CREATE INDEX idx_tenant_models_tenant_id_created_at
    ON model.tenant_models (tenant_id, created_at DESC);

CREATE INDEX idx_tenant_models_workspace_id
    ON model.tenant_models (workspace_id) WHERE workspace_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON model.tenant_models TO api_runtime;

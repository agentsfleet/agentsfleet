-- The tenant's default model. Keyed by tenant (an identity exception: at most one
-- per tenant). `tenant_model_id` NULL means the platform default.
--
-- The composite reference refuses a pointer to another tenant's model. Deleting
-- the model clears only the pointer, falling back to the platform default.

CREATE TABLE model.tenant_selections (
    tenant_id        UUID PRIMARY KEY REFERENCES identity.tenants(id) ON DELETE CASCADE,
    tenant_model_id  UUID,
    created_at       TIMESTAMPTZ(3) NOT NULL,
    updated_at       TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT fk_tenant_selections_tenant_model
        FOREIGN KEY (tenant_model_id, tenant_id)
        REFERENCES model.tenant_models (id, tenant_id)
        ON DELETE SET NULL (tenant_model_id)
);

CREATE INDEX idx_tenant_selections_tenant_model_id
    ON model.tenant_selections (tenant_model_id) WHERE tenant_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON model.tenant_selections TO api_runtime;

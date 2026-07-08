-- Tenant-scoped model registry. Rows represent configured models; credentials
-- stay in vault.secrets and are referenced by key name through secret_ref.
-- Provider labels, base_url, kind, and api_key remain vault JSON metadata, not
-- table columns, so one stored key can back many model rows.

CREATE TABLE IF NOT EXISTS core.tenant_model_entries (
    uid          UUID GENERATED ALWAYS AS (id) STORED PRIMARY KEY,
    CONSTRAINT ck_tenant_model_entries_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    id           UUID NOT NULL UNIQUE,
    tenant_id    UUID NOT NULL REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    model_id     TEXT NOT NULL,
    secret_ref   TEXT NOT NULL,
    created_at   BIGINT NOT NULL,
    updated_at   BIGINT NOT NULL,
    CONSTRAINT uq_tenant_model_entries_entry UNIQUE (tenant_id, model_id, secret_ref)
);

-- api_runtime backs /v1/tenants/me/models list/create/edit/delete.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.tenant_model_entries TO api_runtime;

CREATE INDEX IF NOT EXISTS idx_tenant_model_entries_tenant_created_at
    ON core.tenant_model_entries(tenant_id, created_at DESC);

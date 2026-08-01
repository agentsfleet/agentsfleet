-- Tenant-scoped model registry. A row is a configured model; credentials stay in
-- `vault.secrets` and are referenced by key name through `secret_ref` (RULE VLT).
-- Provider labels, base URL, kind and API key remain vault metadata rather than
-- columns here, so one stored key can back many model rows.
--
-- Unlike `core.tenant_model_selection`, this table holds many rows per tenant,
-- so it carries its own minted identity rather than being keyed by its parent.

CREATE TABLE IF NOT EXISTS core.tenant_model_entries (
    id          UUID   PRIMARY KEY,
    CONSTRAINT ck_tenant_model_entries_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    tenant_id   UUID   NOT NULL REFERENCES core.tenants(id) ON DELETE CASCADE,
    model_id    TEXT   NOT NULL,
    secret_ref  TEXT   NOT NULL,
    created_at  BIGINT NOT NULL,
    updated_at  BIGINT NOT NULL,
    -- The same model backed by a different stored credential is a different
    -- entry, so the domain key is all three columns. It holds a different value
    -- from `id` rather than duplicating it.
    CONSTRAINT uq_tenant_model_entries_tenant_id_model_id_secret_ref
        UNIQUE (tenant_id, model_id, secret_ref)
);

-- Reader: the tenant model list (GET /v1/tenants/me/models), filtered by tenant
-- and ordered newest-first. The unique constraint above leads with `tenant_id`
-- and so serves the filter, but stops before `created_at` — this index carries
-- the sort, so the list is one seek rather than a seek plus a sort node.
CREATE INDEX IF NOT EXISTS idx_tenant_model_entries_tenant_id_created_at
    ON core.tenant_model_entries (tenant_id, created_at DESC);

-- api_runtime backs /v1/tenants/me/models list/create/edit/delete.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.tenant_model_entries TO api_runtime;

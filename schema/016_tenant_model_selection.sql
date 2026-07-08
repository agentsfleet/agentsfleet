-- Tenant-scoped LLM provider configuration. One row per tenant who has
-- explicitly configured a provider; absence of row is the synthesised
-- platform default.
--
-- The resolver (src/state/tenant_provider.zig) treats "no row" and
-- "row with mode=platform" as semantically identical for runtime behaviour.
-- An explicit row is written when the user runs `tenant provider reset`
-- so the dashboard can distinguish "never configured" from
-- "explicitly reset".
--
-- Value constraints (mode ∈ {platform, self_managed}; secret_ref nullability
-- tied to mode) are enforced in application code via constants in
-- src/state/tenant_provider.zig — RULE STS forbids static-string CHECKs.

CREATE TABLE IF NOT EXISTS core.tenant_model_selection (
    uid                UUID    GENERATED ALWAYS AS (tenant_id) STORED PRIMARY KEY,
    CONSTRAINT ck_tenant_model_selection_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    tenant_id          UUID    NOT NULL UNIQUE
                               REFERENCES core.tenants(tenant_id)
                               ON DELETE CASCADE,
    mode               TEXT    NOT NULL,
    provider           TEXT    NOT NULL,
    model              TEXT    NOT NULL,
    context_cap_tokens INTEGER NOT NULL,
    secret_ref         TEXT,
    created_at         BIGINT  NOT NULL,
    updated_at         BIGINT  NOT NULL
);

-- Operator query: list all self-managed-key tenants for support / debugging.
CREATE INDEX IF NOT EXISTS idx_tenant_model_selection_mode
    ON core.tenant_model_selection (mode);

-- api_runtime: GET/PUT/DELETE /v1/tenants/me/provider, plus resolveActiveProvider
-- at lease issue (runs in agentsfleetd post-cutover).
GRANT SELECT, INSERT, UPDATE, DELETE ON core.tenant_model_selection TO api_runtime;

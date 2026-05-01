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
-- Value constraints (mode ∈ {platform, byok}; credential_ref nullability
-- tied to mode) are enforced in application code via constants in
-- src/state/tenant_provider.zig — RULE STS forbids static-string CHECKs.

CREATE TABLE IF NOT EXISTS core.tenant_providers (
    tenant_id          UUID    PRIMARY KEY
                               REFERENCES core.tenants(tenant_id)
                               ON DELETE CASCADE,
    mode               TEXT    NOT NULL,
    provider           TEXT    NOT NULL,
    model              TEXT    NOT NULL,
    context_cap_tokens INTEGER NOT NULL,
    credential_ref     TEXT,
    created_at         BIGINT  NOT NULL,
    updated_at         BIGINT  NOT NULL
);

-- Operator query: list all BYOK tenants for support / debugging.
CREATE INDEX IF NOT EXISTS idx_tenant_providers_mode
    ON core.tenant_providers (mode);

-- api_runtime: GET/PUT/DELETE /v1/tenants/me/provider.
GRANT SELECT, INSERT, UPDATE, DELETE ON core.tenant_providers TO api_runtime;

-- worker_runtime: resolveActiveProvider reads on every event during processEvent.
GRANT SELECT ON core.tenant_providers TO worker_runtime;

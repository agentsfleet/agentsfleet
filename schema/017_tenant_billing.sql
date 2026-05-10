-- Tenant-scoped billing: one row per tenant carries the credit-pool balance.
-- Two-rate metering, both rates expressed in nanos (1/1,000,000,000 USD): events
-- are free both postures; stages cost $0.001 platform / $0.0001 self-managed.
-- No plan tiers. Constants live in src/state/tenant_billing.zig.

CREATE TABLE IF NOT EXISTS billing.tenant_billing (
    tenant_id             UUID PRIMARY KEY REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    balance_nanos         BIGINT NOT NULL CHECK (balance_nanos >= 0),
    grant_source          TEXT   NOT NULL,
    balance_exhausted_at  BIGINT NULL,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tenant_billing_updated
    ON billing.tenant_billing (updated_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.tenant_billing TO api_runtime;
GRANT SELECT, UPDATE                 ON billing.tenant_billing TO worker_runtime;

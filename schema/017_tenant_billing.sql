-- Tenant-scoped billing: one row per tenant carries the credit-pool balance.
-- Single-rate metering ($0.01 event receipt + $0.10 stage); no plan tiers.

CREATE TABLE IF NOT EXISTS billing.tenant_billing (
    tenant_id             UUID PRIMARY KEY REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    balance_cents         BIGINT NOT NULL CHECK (balance_cents >= 0),
    grant_source          TEXT   NOT NULL,
    balance_exhausted_at  BIGINT NULL,
    created_at            BIGINT NOT NULL,
    updated_at            BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tenant_billing_updated
    ON billing.tenant_billing (updated_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.tenant_billing TO api_runtime;
GRANT SELECT, UPDATE                 ON billing.tenant_billing TO worker_runtime;

-- Tenant-scoped billing: one row per tenant carries plan + free-credit balance.
-- Replaces per-workspace billing_state + free_credit (pre-v2.0 teardown).

CREATE TABLE IF NOT EXISTS billing.tenant_billing (
    tenant_id      UUID PRIMARY KEY REFERENCES core.tenants(tenant_id) ON DELETE CASCADE,
    plan_tier      TEXT   NOT NULL,
    plan_sku       TEXT   NOT NULL,
    balance_cents  BIGINT NOT NULL CHECK (balance_cents >= 0),
    grant_source   TEXT   NOT NULL,
    created_at     BIGINT NOT NULL,
    updated_at     BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_tenant_billing_plan
    ON billing.tenant_billing (plan_tier, updated_at DESC);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.tenant_billing TO api_runtime;
GRANT SELECT, UPDATE                 ON billing.tenant_billing TO worker_runtime;

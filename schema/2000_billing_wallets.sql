-- The tenant's balance: the only row money moves on. Keyed by tenant (an identity
-- exception: one per tenant); erasing the tenant removes it (deletion matrix:
-- Tenant). A settlement takes its whole charge, so a balance that cannot cover
-- one goes below zero and the debt is repaid by the next grant before any of it
-- is spendable. `exhausted_at` is the instant the balance reached zero or below,
-- returned by the billing page.

CREATE TABLE billing.wallets (
    tenant_id      UUID PRIMARY KEY REFERENCES identity.tenants(id) ON DELETE CASCADE,
    balance_nanos  BIGINT NOT NULL,
    exhausted_at   TIMESTAMPTZ(3),
    created_at     TIMESTAMPTZ(3) NOT NULL,
    updated_at     TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.wallets TO api_runtime;

-- The tenant's balance: the only row money moves on. Keyed by tenant (an identity
-- exception: one per tenant); erasing the tenant removes it (deletion matrix:
-- Tenant). `exhausted_at` is the instant the balance hit zero, returned by the
-- billing page and read by admission. The balance can never go negative.

CREATE TABLE billing.wallets (
    tenant_id      UUID PRIMARY KEY REFERENCES identity.tenants(id) ON DELETE CASCADE,
    balance_nanos  BIGINT NOT NULL,
    exhausted_at   TIMESTAMPTZ(3),
    created_at     TIMESTAMPTZ(3) NOT NULL,
    updated_at     TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_wallets_balance_nanos_non_negative CHECK (balance_nanos >= 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.wallets TO api_runtime;

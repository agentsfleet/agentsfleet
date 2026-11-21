-- The only row money moves on; amounts are nanodollars. A settlement takes its whole charge, so
-- the balance can go negative, and the next grant repays that debt first. `exhausted_at` is when
-- the balance reached zero or below.
CREATE TABLE billing.wallets (
    tenant_id      UUID PRIMARY KEY REFERENCES identity.tenants(id) ON DELETE CASCADE,
    balance_nanos  BIGINT NOT NULL,
    exhausted_at   TIMESTAMPTZ(3),
    created_at     TIMESTAMPTZ(3) NOT NULL,
    updated_at     TIMESTAMPTZ(3) NOT NULL
);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.wallets TO api_runtime;

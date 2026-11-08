INSERT INTO billing.wallets (tenant_id, balance_nanos, created_at, updated_at)
VALUES ($1::uuid, $2, now(), now())
ON CONFLICT (tenant_id) DO NOTHING

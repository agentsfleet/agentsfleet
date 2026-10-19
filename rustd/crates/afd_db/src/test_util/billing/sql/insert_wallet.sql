INSERT INTO billing.wallets (tenant_id, balance_nanos, created_at, updated_at)
VALUES ($1::uuid, $2, coalesce($3::timestamptz, now()), coalesce($3::timestamptz, now()))

SELECT balance_nanos, exhausted_at, updated_at
FROM billing.wallets
WHERE tenant_id = $1::uuid

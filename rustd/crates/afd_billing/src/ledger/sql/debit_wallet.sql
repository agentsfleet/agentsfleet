UPDATE billing.wallets
SET balance_nanos = GREATEST(balance_nanos - $2, 0),
    exhausted_at = CASE WHEN balance_nanos - $2 <= 0 THEN COALESCE(exhausted_at, now())
                        ELSE exhausted_at END,
    updated_at = now()
WHERE tenant_id = $1::uuid

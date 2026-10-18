WITH before AS (
    SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1::uuid
)
UPDATE billing.wallets w
SET balance_nanos = GREATEST(w.balance_nanos - $2, 0),
    exhausted_at = CASE WHEN w.balance_nanos - $2 <= 0 THEN COALESCE(w.exhausted_at, now())
                        ELSE w.exhausted_at END,
    updated_at = now()
FROM before b
WHERE w.tenant_id = $1::uuid
RETURNING LEAST(b.balance_nanos, $2)::bigint AS taken

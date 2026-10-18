WITH charge AS (
    SELECT LEAST(balance_nanos, $2::bigint) AS amount
      FROM billing.wallets
     WHERE tenant_id = $1::uuid
)
UPDATE billing.wallets w
   SET balance_nanos = w.balance_nanos - c.amount, updated_at = now()
  FROM charge c
 WHERE w.tenant_id = $1::uuid
RETURNING c.amount

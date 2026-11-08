SELECT w.balance_nanos, w.exhausted_at,
       COALESCE((SELECT SUM(u.charged_nanos)
                 FROM billing.usage_lines u
                 WHERE u.tenant_id = w.tenant_id AND u.settled_at IS NULL), 0)::bigint AS unsettled
FROM billing.wallets w
WHERE w.tenant_id = $1

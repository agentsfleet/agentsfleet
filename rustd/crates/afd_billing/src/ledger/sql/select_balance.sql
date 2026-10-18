SELECT w.balance_nanos, w.exhausted_at,
       COALESCE((SELECT SUM(GREATEST(h.ceiling_nanos - h.captured_nanos, 0))
                 FROM billing.holds h
                 WHERE h.tenant_id = w.tenant_id AND h.released_at IS NULL), 0)::bigint AS held
FROM billing.wallets w
WHERE w.tenant_id = $1::uuid

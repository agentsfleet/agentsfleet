SELECT tenant_id::text FROM billing.wallets WHERE tenant_id = $1::uuid FOR UPDATE

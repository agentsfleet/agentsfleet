SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1::uuid FOR UPDATE

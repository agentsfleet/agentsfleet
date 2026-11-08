SELECT balance_nanos FROM billing.wallets WHERE tenant_id = $1 FOR UPDATE

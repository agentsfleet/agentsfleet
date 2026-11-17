SELECT COUNT(*)::bigint FROM runner.runners WHERE ($1::uuid IS NULL OR tenant_id = $1)

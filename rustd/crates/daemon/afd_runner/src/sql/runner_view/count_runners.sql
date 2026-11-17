SELECT COUNT(*)::bigint FROM runner.runners WHERE ($2::boolean OR tenant_id = $1::uuid)

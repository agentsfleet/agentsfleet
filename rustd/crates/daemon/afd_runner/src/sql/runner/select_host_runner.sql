SELECT id FROM runner.runners WHERE tenant_id IS NOT DISTINCT FROM $1::uuid AND host_id = $2::text

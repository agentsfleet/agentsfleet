SELECT count(*) FROM setup.attempts WHERE tenant_id = ANY($1)

SELECT count(*) FROM authoring.attempts WHERE tenant_id = ANY($1::uuid[])

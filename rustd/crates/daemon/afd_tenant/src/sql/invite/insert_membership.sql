INSERT INTO identity.memberships (id, tenant_id, user_id, role, created_at)
VALUES ($1, $2, $3, $4, now())
ON CONFLICT (tenant_id, user_id) DO NOTHING

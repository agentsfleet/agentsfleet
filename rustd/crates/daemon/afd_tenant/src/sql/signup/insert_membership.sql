INSERT INTO identity.memberships (id, tenant_id, user_id, role, created_at)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, now())

INSERT INTO identity.workspaces (id, tenant_id, name, created_at)
VALUES ($1::uuid, $2::uuid, $3, now())

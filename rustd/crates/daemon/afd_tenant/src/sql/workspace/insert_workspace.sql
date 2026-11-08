INSERT INTO identity.workspaces (id, tenant_id, name, created_at)
VALUES ($1, $2, $3, now())

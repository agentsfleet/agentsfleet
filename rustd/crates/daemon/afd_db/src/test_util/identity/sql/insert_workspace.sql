INSERT INTO identity.workspaces (id, tenant_id, name, created_at)
VALUES ($1, $2, $3, coalesce($4::timestamptz, now()))

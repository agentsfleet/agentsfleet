SELECT EXISTS (SELECT 1 FROM identity.workspaces WHERE tenant_id = $1)

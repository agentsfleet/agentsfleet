SELECT tenant_id::text
FROM identity.workspaces
WHERE id = $1::uuid

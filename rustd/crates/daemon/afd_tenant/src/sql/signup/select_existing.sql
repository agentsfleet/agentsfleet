SELECT u.id::text AS user_id, t.id::text AS tenant_id, w.id::text AS workspace_id,
       w.name AS workspace_name
FROM identity.users u
JOIN identity.memberships m ON m.user_id = u.id AND m.role = $2
JOIN identity.tenants t ON t.id = m.tenant_id
JOIN identity.workspaces w ON w.tenant_id = t.id AND w.name IS NOT NULL
WHERE u.oidc_subject = $1
ORDER BY w.created_at ASC, w.id ASC
LIMIT 1

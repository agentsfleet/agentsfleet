SELECT w.tenant_id::text, m.role
FROM identity.workspaces w
LEFT JOIN identity.users u ON u.oidc_subject = $2
LEFT JOIN identity.memberships m ON m.tenant_id = w.tenant_id AND m.user_id = u.id
WHERE w.id = $1::uuid
  AND (m.id IS NOT NULL OR w.tenant_id = COALESCE(u.tenant_id, $3::uuid))

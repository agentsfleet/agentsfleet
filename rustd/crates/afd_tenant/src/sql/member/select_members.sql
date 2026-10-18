SELECT u.id::text AS user_id, u.display_name, u.email, u.oidc_subject, m.role,
       m.created_at AS joined_at
FROM identity.memberships m
JOIN identity.users u ON u.id = m.user_id
WHERE m.tenant_id = $1::uuid
ORDER BY m.created_at, m.id

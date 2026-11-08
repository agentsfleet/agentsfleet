SELECT 1 FROM identity.memberships m
JOIN identity.users u ON u.id = m.user_id
WHERE m.tenant_id = $1::uuid AND lower(u.email) = $2
UNION ALL
SELECT 1 FROM identity.users u WHERE u.tenant_id = $1::uuid AND lower(u.email) = $2
LIMIT 1

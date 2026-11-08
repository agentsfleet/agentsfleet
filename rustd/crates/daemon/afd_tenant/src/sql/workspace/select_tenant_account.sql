SELECT t.id::text AS tenant_id, NULL::text AS role,
       COALESCE(owner.display_name, t.name) AS owner_name,
       t.id::text AS home_tenant_id
FROM identity.tenants t
LEFT JOIN LATERAL (
    SELECT u.display_name FROM identity.memberships om
    JOIN identity.users u ON u.id = om.user_id
    WHERE om.tenant_id = t.id AND om.role = $2 AND u.display_name IS NOT NULL
    ORDER BY om.created_at, om.id LIMIT 1
) owner ON TRUE
WHERE t.id = $1::uuid

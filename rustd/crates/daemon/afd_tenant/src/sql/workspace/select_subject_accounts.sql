WITH me AS (
    SELECT id, tenant_id FROM identity.users WHERE oidc_subject = $1
), held AS (
    SELECT DISTINCT ON (arm.tenant_id) arm.tenant_id, arm.role FROM (
        SELECT m.tenant_id, m.role FROM identity.memberships m JOIN me ON m.user_id = me.id
        UNION ALL
        SELECT me.tenant_id, NULL::text FROM me
    ) arm ORDER BY arm.tenant_id, arm.role NULLS LAST
)
SELECT t.id::text AS tenant_id, held.role,
       COALESCE(owner.display_name, t.name) AS owner_name,
       me.tenant_id::text AS home_tenant_id
FROM held CROSS JOIN me
JOIN identity.tenants t ON t.id = held.tenant_id
LEFT JOIN LATERAL (
    SELECT u.display_name FROM identity.memberships om
    JOIN identity.users u ON u.id = om.user_id
    WHERE om.tenant_id = t.id AND om.role = $2 AND u.display_name IS NOT NULL
    ORDER BY om.created_at, om.id LIMIT 1
) owner ON TRUE

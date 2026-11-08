SELECT i.id::text AS id, t.id::text AS tenant_id,
       COALESCE(owner.display_name, t.name) AS owner_name,
       i.expires_at
FROM identity.invites i
JOIN identity.tenants t ON t.id = i.tenant_id
LEFT JOIN LATERAL (
    SELECT u.display_name FROM identity.memberships om
    JOIN identity.users u ON u.id = om.user_id
    WHERE om.tenant_id = t.id AND om.role = $2 AND u.display_name IS NOT NULL
    ORDER BY om.created_at, om.id LIMIT 1
) owner ON TRUE
WHERE i.email = $1 AND i.resolved_at IS NULL AND i.expires_at > statement_timestamp()
ORDER BY i.created_at DESC, i.id DESC

WITH target AS MATERIALIZED (
    SELECT id, expires_at
    FROM identity.invites
    WHERE tenant_id = $1::uuid AND id = $2::uuid AND resolved_at IS NULL
    FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS locked_at FROM target
), counted AS (
    UPDATE identity.invites i
    SET email_attempt_count = i.email_attempt_count + 1, updated_at = now()
    FROM target CROSS JOIN clock
    WHERE i.id = target.id AND target.expires_at > clock.locked_at
    RETURNING i.tenant_id, i.email, i.invited_by_user_id, i.email_attempt_count
)
SELECT c.email_attempt_count, c.email,
       COALESCE(inviter.display_name, inviter.email) AS inviter_name,
       COALESCE(owner.display_name, t.name) AS owner_name
FROM counted c
JOIN identity.tenants t ON t.id = c.tenant_id
JOIN identity.users inviter ON inviter.id = c.invited_by_user_id
LEFT JOIN LATERAL (
    SELECT u.display_name FROM identity.memberships om
    JOIN identity.users u ON u.id = om.user_id
    WHERE om.tenant_id = t.id AND om.role = $3 AND u.display_name IS NOT NULL
    ORDER BY om.created_at, om.id LIMIT 1
) owner ON TRUE

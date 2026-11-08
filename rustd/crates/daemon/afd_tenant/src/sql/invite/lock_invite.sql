WITH locked AS MATERIALIZED (
    SELECT id, tenant_id, email, role, expires_at, created_at, accepted_by_user_id,
           resolved_at, email_status, email_sent_at
    FROM identity.invites
    WHERE id = $1
    FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS locked_at FROM locked
)
SELECT locked.id AS id, locked.tenant_id AS tenant_id, locked.email, locked.role,
       locked.expires_at, locked.created_at,
       locked.accepted_by_user_id AS accepted_by_user_id, locked.resolved_at,
       locked.email_status, locked.email_sent_at, clock.locked_at
FROM locked CROSS JOIN clock

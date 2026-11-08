SELECT id, tenant_id, email, role, expires_at, created_at,
       accepted_by_user_id, resolved_at, email_status, email_sent_at
FROM identity.invites
WHERE tenant_id = $1 AND resolved_at IS NULL AND expires_at > statement_timestamp()
ORDER BY created_at DESC, id DESC

SELECT c.tenant_id::text AS tenant, u.oidc_subject AS subject, c.revoked_at IS NULL AS usable
FROM identity.cli_credentials c
JOIN identity.users u ON u.id = c.user_id
WHERE c.credential_hash = $1
LIMIT 1

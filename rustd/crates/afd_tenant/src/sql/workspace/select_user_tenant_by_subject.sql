SELECT tenant_id::text
FROM identity.users
WHERE oidc_subject = $1
LIMIT 1

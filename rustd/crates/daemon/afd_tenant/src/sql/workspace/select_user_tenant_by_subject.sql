SELECT tenant_id
FROM identity.users
WHERE oidc_subject = $1
LIMIT 1

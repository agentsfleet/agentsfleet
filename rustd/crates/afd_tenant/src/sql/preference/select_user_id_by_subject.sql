SELECT id::text
FROM identity.users
WHERE oidc_subject = $1

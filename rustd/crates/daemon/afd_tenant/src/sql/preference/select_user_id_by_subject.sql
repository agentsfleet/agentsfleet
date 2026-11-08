SELECT id
FROM identity.users
WHERE oidc_subject = $1

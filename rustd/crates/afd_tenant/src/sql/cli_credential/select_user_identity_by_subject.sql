SELECT users.id::text AS id, users.tenant_id::text AS tenant_id, users.email, users.display_name,
       tenants.name AS tenant_name
FROM identity.users AS users
JOIN identity.tenants AS tenants ON tenants.id = users.tenant_id
WHERE users.oidc_subject = $1
LIMIT 1

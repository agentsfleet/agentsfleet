SELECT count(*) FROM identity.memberships
WHERE tenant_id = $1 AND role = $2

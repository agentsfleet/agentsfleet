SELECT count(*) FROM identity.memberships
WHERE tenant_id = $1::uuid AND role = $2

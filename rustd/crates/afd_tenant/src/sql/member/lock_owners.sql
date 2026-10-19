SELECT 1 FROM identity.memberships
WHERE tenant_id = $1::uuid AND role = $2
ORDER BY id
FOR UPDATE

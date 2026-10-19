SELECT role FROM identity.memberships
WHERE tenant_id = $1::uuid AND user_id = $2::uuid
FOR UPDATE

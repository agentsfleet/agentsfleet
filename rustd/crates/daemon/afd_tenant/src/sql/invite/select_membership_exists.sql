SELECT 1 FROM identity.memberships WHERE tenant_id = $1::uuid AND user_id = $2::uuid

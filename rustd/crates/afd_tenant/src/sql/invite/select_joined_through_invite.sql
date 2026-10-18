SELECT 1 FROM identity.invites i
JOIN identity.memberships m ON m.tenant_id = i.tenant_id AND m.user_id = i.accepted_by_user_id
WHERE i.tenant_id = $1::uuid AND i.id = $2::uuid

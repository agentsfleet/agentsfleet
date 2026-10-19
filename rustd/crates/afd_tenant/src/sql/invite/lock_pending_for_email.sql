SELECT 1 FROM identity.invites
WHERE tenant_id = $1::uuid AND email = $2 AND resolved_at IS NULL
FOR UPDATE

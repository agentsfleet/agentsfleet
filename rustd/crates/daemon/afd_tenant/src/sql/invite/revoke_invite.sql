UPDATE identity.invites
SET status = $3, resolved_at = now(), updated_at = now()
WHERE tenant_id = $1::uuid AND id = $2::uuid AND resolved_at IS NULL

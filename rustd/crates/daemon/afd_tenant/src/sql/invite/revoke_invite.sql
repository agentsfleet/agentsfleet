UPDATE identity.invites
SET status = $3, resolved_at = now(), updated_at = now()
WHERE tenant_id = $1 AND id = $2 AND resolved_at IS NULL

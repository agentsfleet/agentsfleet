UPDATE identity.invites
SET status = $3, resolved_at = now(), updated_at = now()
WHERE tenant_id = $1::uuid AND email = $2 AND resolved_at IS NULL
  AND expires_at <= clock_timestamp()

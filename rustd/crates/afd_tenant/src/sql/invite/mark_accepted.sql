UPDATE identity.invites
SET status = $3, resolved_at = now(), accepted_by_user_id = $2::uuid, updated_at = now()
WHERE id = $1::uuid AND resolved_at IS NULL

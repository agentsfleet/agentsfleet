UPDATE identity.invites
SET status = $3, resolved_at = now(), accepted_by_user_id = $2, updated_at = now()
WHERE id = $1 AND resolved_at IS NULL

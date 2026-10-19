UPDATE identity.cli_credentials
SET revoked_at = now(), updated_at = now()
WHERE id = $1::uuid AND user_id = $2::uuid AND revoked_at IS NULL
RETURNING revoked_at

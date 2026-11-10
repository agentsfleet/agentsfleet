UPDATE identity.cli_credentials
SET revoked_at = now(), updated_at = now()
WHERE user_id = $1 AND machine_name = $2 AND revoked_at IS NULL

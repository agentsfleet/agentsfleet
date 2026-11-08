SELECT status, cli_public_key, token_name, expires_at
FROM identity.device_sessions
WHERE id = $1
  AND expires_at > statement_timestamp()

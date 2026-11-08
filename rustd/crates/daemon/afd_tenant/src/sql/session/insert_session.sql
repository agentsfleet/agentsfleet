INSERT INTO identity.device_sessions
    (id, status, cli_public_key, token_name, verification_attempts, created_at, updated_at, expires_at)
VALUES ($1, $2, $3, $4, 0, now(), now(), clock_timestamp() + $5::interval)
RETURNING expires_at

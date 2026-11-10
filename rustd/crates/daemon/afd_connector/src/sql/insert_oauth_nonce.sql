INSERT INTO connector.oauth_nonces (id, provider, nonce_digest, created_at, expires_at)
VALUES ($1, $2, $3, now(), clock_timestamp() + $4::interval)

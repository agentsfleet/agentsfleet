-- The DELETE takes the row lock; the clock is read only after it, so a nonce
-- that expired while a competing spend held the row answers nobody. An
-- expired nonce presented here is removed too: nothing can ever spend it.
WITH spent AS (
    DELETE FROM connector.oauth_nonces
    WHERE provider = $1
      AND nonce_digest = $2
    RETURNING id, expires_at
)
SELECT id FROM spent WHERE expires_at > clock_timestamp()

DELETE FROM connector.oauth_nonces
WHERE id IN (
    SELECT id
    FROM connector.oauth_nonces
    WHERE expires_at <= statement_timestamp()
    ORDER BY expires_at
    LIMIT $1
)

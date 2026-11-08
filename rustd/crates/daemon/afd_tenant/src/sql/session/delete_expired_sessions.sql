DELETE FROM identity.device_sessions
WHERE id IN (
    SELECT id
    FROM identity.device_sessions
    WHERE expires_at <= statement_timestamp()
    ORDER BY expires_at
    LIMIT $1
    FOR UPDATE SKIP LOCKED
)

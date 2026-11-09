UPDATE identity.device_sessions
SET status = $2,
    replay_expires_at = $3::timestamptz + $4::interval,
    consumed_client_fingerprint = $5,
    expires_at = clock_timestamp() + $6::interval,
    updated_at = now()
WHERE id = $1

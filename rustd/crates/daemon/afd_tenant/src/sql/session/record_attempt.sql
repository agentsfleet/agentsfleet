UPDATE identity.device_sessions
SET verification_attempt_count = $2,
    status = $3,
    aborted_reason = $4,
    expires_at = clock_timestamp() + $5::interval,
    updated_at = now()
WHERE id = $1

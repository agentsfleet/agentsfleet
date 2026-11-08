WITH locked AS MATERIALIZED (
    SELECT id, status, dashboard_public_key, ciphertext, nonce, verification_code_hmac,
           verification_attempts, aborted_reason, consumed_client_fingerprint,
           expires_at, replay_expires_at
    FROM identity.device_sessions
    WHERE id = $1
    FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS now FROM locked
)
SELECT locked.status, locked.dashboard_public_key, locked.ciphertext, locked.nonce,
       locked.verification_code_hmac, locked.verification_attempts, locked.aborted_reason,
       locked.consumed_client_fingerprint, locked.expires_at, locked.replay_expires_at, clock.now
FROM locked, clock

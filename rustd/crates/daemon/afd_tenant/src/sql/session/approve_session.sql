WITH locked AS MATERIALIZED (
    SELECT id
    FROM identity.device_sessions
    WHERE id = $1
      AND status = $2
    FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS now FROM locked
)
UPDATE identity.device_sessions AS s
SET status = $3,
    dashboard_public_key = $4,
    ciphertext = $5,
    nonce = $6,
    verification_code_hmac = $7,
    approver_subject = $8,
    approved_at = clock.now,
    expires_at = clock.now + $9::interval,
    updated_at = now()
FROM locked, clock
WHERE s.id = locked.id
  AND s.expires_at > clock.now
RETURNING s.id

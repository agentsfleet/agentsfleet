UPDATE identity.device_sessions
SET status = $3,
    dashboard_public_key = $4,
    ciphertext = $5,
    nonce = $6,
    verification_code_hmac = $7,
    approver_subject = $8,
    approved_at = clock_timestamp(),
    expires_at = clock_timestamp() + $9::interval,
    updated_at = now()
WHERE id = $1
  AND status = $2
  AND expires_at > clock_timestamp()
RETURNING id

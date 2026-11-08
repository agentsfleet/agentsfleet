UPDATE identity.device_sessions
SET status = $3,
    aborted_reason = $4,
    expires_at = clock_timestamp() + $5::interval,
    updated_at = now()
WHERE approver_subject = $1
  AND status <> ALL($2)
  AND expires_at > clock_timestamp()
RETURNING id

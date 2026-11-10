SELECT status
FROM identity.device_sessions
WHERE id = $1
  AND expires_at > statement_timestamp()

WITH locked AS MATERIALIZED (
    SELECT id
    FROM identity.device_sessions
    WHERE approver_subject = $1
      AND status <> ALL($2)
    FOR UPDATE
), clock AS MATERIALIZED (
    SELECT clock_timestamp() AS now FROM (SELECT 1 FROM locked LIMIT 1) AS first_lock
)
UPDATE identity.device_sessions AS s
SET status = $3,
    aborted_reason = $4,
    expires_at = clock.now + $5::interval,
    updated_at = now()
FROM locked, clock
WHERE s.id = locked.id
  AND s.expires_at > clock.now
RETURNING s.id

WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), prior AS (
  SELECT s.hold_expires_at > instant.checked_at AS held, s.last_runner_id
  FROM runner.fleet_slots s, instant
  WHERE s.fleet_id = $1::uuid
)
INSERT INTO runner.fleet_slots AS s
  (fleet_id, last_runner_id, fencing_token, lease_expires_at, hold_expires_at,
   created_at, updated_at)
SELECT $1::uuid, $2::uuid, 1, instant.checked_at + $3::interval, NULL, now(), now()
FROM instant
ON CONFLICT (fleet_id) DO UPDATE
  SET last_runner_id   = EXCLUDED.last_runner_id,
      fencing_token    = s.fencing_token + 1,
      lease_expires_at = EXCLUDED.lease_expires_at,
      hold_expires_at  = CASE WHEN s.last_runner_id = EXCLUDED.last_runner_id
                              THEN s.hold_expires_at END,
      updated_at       = now()
  WHERE s.lease_expires_at < (SELECT checked_at FROM instant)
    AND (s.hold_expires_at IS NULL
         OR s.hold_expires_at <= (SELECT checked_at FROM instant)
         OR s.last_runner_id = $2::uuid
         OR NOT EXISTS (
              SELECT 1 FROM runner.runners r
              WHERE r.id = s.last_runner_id
                AND r.last_seen_at > (SELECT checked_at FROM instant) - $4::interval
                AND r.status = $5
                AND r.degraded_reason IS NULL))
RETURNING s.fencing_token,
          s.lease_expires_at,
          COALESCE((SELECT held FROM prior), false) AS held,
          (SELECT last_runner_id::text FROM prior) AS holder

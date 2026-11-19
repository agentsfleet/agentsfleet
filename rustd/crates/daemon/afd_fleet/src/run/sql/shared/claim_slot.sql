-- The slot claim both kinds share. A slot's id is its owner's, so the owner's one
-- non-NULL column is the key, and a bind that names both owners or neither fails.
-- $1 the owning fleet, NULL for a draft · $2 the owning draft, NULL for a fleet ·
-- $3 runner · $4 run lifetime · $5 how long a silent runner stays live ·
-- $6 the runner status that may run
WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), prior AS (
  SELECT s.hold_expires_at > instant.checked_at AS held, s.last_runner_id
  FROM runner.slots s, instant
  WHERE s.id = COALESCE($1::uuid, $2::uuid)
)
INSERT INTO runner.slots AS s
  (id, fleet_id, draft_id, last_runner_id, fencing_token, run_expires_at, hold_expires_at,
   created_at, updated_at)
SELECT COALESCE($1::uuid, $2::uuid), $1::uuid, $2::uuid, $3, 1,
       instant.checked_at + $4::interval, NULL, now(), now()
FROM instant
ON CONFLICT (id) DO UPDATE
  SET last_runner_id   = EXCLUDED.last_runner_id,
      fencing_token    = s.fencing_token + 1,
      run_expires_at = EXCLUDED.run_expires_at,
      hold_expires_at  = CASE WHEN s.last_runner_id = EXCLUDED.last_runner_id
                              THEN s.hold_expires_at END,
      updated_at       = now()
  WHERE s.run_expires_at < (SELECT checked_at FROM instant)
    AND (s.hold_expires_at IS NULL
         OR s.hold_expires_at <= (SELECT checked_at FROM instant)
         OR s.last_runner_id = $3
         OR NOT EXISTS (
              SELECT 1 FROM runner.runners r
              WHERE r.id = s.last_runner_id
                AND r.last_seen_at > (SELECT checked_at FROM instant) - $5::interval
                AND r.status = $6
                AND r.degraded_reason IS NULL))
RETURNING s.fencing_token,
          s.run_expires_at,
          COALESCE((SELECT held FROM prior), false) AS held,
          (SELECT last_runner_id FROM prior) AS holder

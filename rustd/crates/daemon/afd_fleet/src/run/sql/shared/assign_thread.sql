-- A fleet's thread assignment alone, as the `won_thread` step of `../run/assign_turn_head.sql`
-- runs it. A thread's id is its fleet's.
-- $1 fleet · $2 runner · $3 run lifetime · $4 how long a silent runner stays live ·
-- $5 the runner status that may run
WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), prior AS (
  SELECT s.hold_expires_at > instant.checked_at AS held, s.last_runner_id
  FROM thread.threads s, instant
  WHERE s.id = $1::uuid
)
INSERT INTO thread.threads AS s
  (id, fleet_id, last_runner_id, fencing_token, run_expires_at, hold_expires_at,
   created_at, updated_at)
SELECT $1::uuid, $1::uuid, $2, 1,
       instant.checked_at + $3::interval, NULL, now(), now()
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
         OR s.last_runner_id = $2
         OR NOT EXISTS (
              SELECT 1 FROM runner.runners r
              WHERE r.id = s.last_runner_id
                AND r.last_seen_at > (SELECT checked_at FROM instant) - $4::interval
                AND r.status = $5
                AND r.degraded_reason IS NULL))
RETURNING s.fencing_token,
          s.run_expires_at,
          COALESCE((SELECT held FROM prior), false) AS held,
          (SELECT last_runner_id FROM prior) AS holder

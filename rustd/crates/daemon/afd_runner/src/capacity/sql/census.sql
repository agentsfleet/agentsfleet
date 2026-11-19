WITH instant AS MATERIALIZED (
  -- Capacity uses the fresh database clock after the host lock; issue returns its own saved timestamp.
  SELECT clock_timestamp() AS checked_at
)
SELECT instant.checked_at,
       (SELECT count(*) FROM runner.runs l
         WHERE l.runner_id = $1 AND l.status = $2
           AND l.run_expires_at > instant.checked_at)
     -- Setup turns the authoring path still runs; this term goes with authoring.attempts.
     + (SELECT count(*) FROM authoring.attempts a
         WHERE a.runner_id = $1 AND a.status = $2
           AND a.run_expires_at > instant.checked_at) AS occupied,
       (SELECT count(*) FROM runner.slots s
         WHERE $4::boolean AND s.last_runner_id = $1
           AND s.hold_expires_at IS NOT NULL
           AND s.hold_expires_at > instant.checked_at
           -- Every other owner's hold, fleet or draft: a slot's id is its owner's.
           AND ($3::uuid IS NULL OR s.id <> $3::uuid)) AS held
FROM instant

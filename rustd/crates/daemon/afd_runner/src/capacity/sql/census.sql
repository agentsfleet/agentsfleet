WITH instant AS MATERIALIZED (
  -- Capacity uses the fresh database clock after the host lock; issue returns its own saved timestamp.
  SELECT clock_timestamp() AS checked_at
)
SELECT instant.checked_at,
       (SELECT count(*) FROM runner.leases l
         WHERE l.runner_id = $1 AND l.status = $2
           AND l.lease_expires_at > instant.checked_at)
     + (SELECT count(*) FROM authoring.attempts a
         WHERE a.runner_id = $1 AND a.status = $2
           AND a.lease_expires_at > instant.checked_at) AS occupied
FROM instant

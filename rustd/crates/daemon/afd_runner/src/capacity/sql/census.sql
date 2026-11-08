WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
)
SELECT instant.checked_at,
       (SELECT count(*) FROM runner.leases l
         WHERE l.runner_id = $1::uuid AND l.status = $2::text
           AND l.lease_expires_at > instant.checked_at)
     + (SELECT count(*) FROM authoring.attempts a
         WHERE a.runner_id = $1::uuid AND a.status = $2::text
           AND a.lease_expires_at > instant.checked_at) AS occupied
FROM instant

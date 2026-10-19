WITH locked AS MATERIALIZED (
  SELECT id, last_seen_at FROM runner.runners WHERE id = $1::uuid FOR UPDATE
), observed AS MATERIALIZED (
  SELECT locked.id, locked.last_seen_at AS previous, clock_timestamp() AS checked_at
  FROM locked
), bumped AS (
  UPDATE runner.runners r
  SET last_seen_at = now(), updated_at = now()
  FROM observed
  WHERE r.id = observed.id
  RETURNING observed.previous, observed.checked_at
)
INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
SELECT $2::uuid, $1::uuid, $3::text,
       jsonb_build_object($4::text,
         regexp_replace(to_char(previous AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS'),
                        '\.?0+$', '') || 'Z'),
       NULL, now()
FROM bumped
WHERE previous = $5::timestamptz OR previous < checked_at - $6::interval

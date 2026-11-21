-- Lock first, then re-read silence against the clock after any wait.
WITH locked AS MATERIALIZED (
  SELECT id, last_seen_at, status, cordon_reason FROM runner.runners
  WHERE id = $1 FOR UPDATE
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS at FROM (SELECT count(*) FROM locked) observed
), silent AS MATERIALIZED (
  SELECT r.*, i.at FROM locked r CROSS JOIN instant i
  WHERE r.last_seen_at <> $2::timestamptz AND r.last_seen_at < i.at - $3::interval
), offline AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $4, id, $5, jsonb_build_object($6::text, regexp_replace(to_char(last_seen_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS'), '\.?0+$', '') || 'Z'), last_seen_at, at
  FROM silent
  ON CONFLICT (runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL DO NOTHING
  RETURNING id
), cordoned AS (
  UPDATE runner.runners r SET cordon_reason = $7, cordoned_at = COALESCE(r.cordoned_at, s.at),
      cordon_healthy_since_at = NULL, updated_at = s.at
  FROM silent s WHERE r.id = s.id AND r.status = $8
  RETURNING r.id
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $9, c.id, $10, jsonb_build_object($11::text, $7::text), NULL, s.at
  FROM cordoned c JOIN silent s ON s.id = c.id WHERE s.cordon_reason IS NULL
  RETURNING id
)
SELECT (SELECT count(*) FROM offline) + (SELECT count(*) FROM audit) AS changed,
       EXISTS(SELECT 1 FROM audit) AS cordoned

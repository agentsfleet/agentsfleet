WITH locked AS MATERIALIZED (
  SELECT id FROM runner.runners WHERE id = $1 AND status = $3::text FOR UPDATE
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS at FROM (SELECT count(*) FROM locked) observed
), updated AS (
  UPDATE runner.runners r
  SET status = $2::text, updated_at = now()
  WHERE r.id = $1 AND r.status = $3::text
    AND EXISTS (SELECT 1 FROM locked)
    AND NOT EXISTS (
      SELECT 1 FROM thread.runs l
      WHERE l.runner_id = r.id AND l.status = $4::text
        AND l.run_expires_at > (SELECT at FROM instant)
    )
    -- Setup turns the authoring path still runs; this term goes with setup.attempts.
    AND NOT EXISTS (
      SELECT 1 FROM setup.attempts a WHERE a.runner_id = r.id AND a.status = $4::text
        AND a.run_expires_at > (SELECT at FROM instant)
    )
  RETURNING r.id
), inserted AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $5, id, $6::text,
         jsonb_build_object($7::text, $3::text, $8::text, $2::text), NULL, now()
  FROM updated
  RETURNING 1
)
SELECT COUNT(*)::bigint FROM inserted

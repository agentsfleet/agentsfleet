WITH current_state AS (
  SELECT id, status AS from_status
  FROM runner.runners
  WHERE id = $1::uuid
  FOR UPDATE
), updated AS (
  UPDATE runner.runners r
  SET status = $2::text, updated_at = now()
  FROM current_state c
  WHERE r.id = c.id
    AND ($3::bool OR c.from_status <> $4::text)
    AND c.from_status <> $2::text
  RETURNING r.id
), event AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $5::uuid, id, $6::text,
         jsonb_build_object($7::text, from_status, $8::text, $2::text, $9::text, $10::text),
         NULL, now()
  FROM current_state
  WHERE EXISTS (SELECT 1 FROM updated)
)
SELECT c.from_status, EXISTS (SELECT 1 FROM updated) AS changed
FROM current_state c

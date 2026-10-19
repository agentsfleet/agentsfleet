WITH current_runner AS (
  SELECT id, status FROM runner.runners WHERE id = $1::uuid FOR UPDATE
), updated AS (
  UPDATE runner.runners r
  SET token_hash = $2::text, updated_at = now()
  FROM current_runner c
  WHERE r.id = c.id AND c.status <> $3::text
  RETURNING r.id
), event AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $4::uuid, id, $5::text, jsonb_build_object($6::text, $7::text), NULL, now()
  FROM updated
  RETURNING id
)
SELECT EXISTS (SELECT 1 FROM updated) AS changed
FROM current_runner

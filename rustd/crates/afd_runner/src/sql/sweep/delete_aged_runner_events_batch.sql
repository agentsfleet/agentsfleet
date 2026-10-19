DELETE FROM runner.events
WHERE id IN (
  SELECT id FROM runner.events
  WHERE event_type = ANY($1::text[]) AND created_at < statement_timestamp() - $2::interval
  LIMIT $3::bigint
  FOR UPDATE SKIP LOCKED
)

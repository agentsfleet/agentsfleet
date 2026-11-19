DELETE FROM thread.runs
WHERE id IN (
  SELECT id FROM thread.runs
  WHERE status = ANY($1::text[]) AND updated_at < statement_timestamp() - $2::interval
  LIMIT $3::bigint
  FOR UPDATE SKIP LOCKED
)

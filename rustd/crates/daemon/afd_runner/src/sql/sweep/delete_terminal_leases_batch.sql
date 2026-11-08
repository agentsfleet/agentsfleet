DELETE FROM runner.leases
WHERE id IN (
  SELECT id FROM runner.leases
  WHERE status = ANY($1::text[]) AND updated_at < statement_timestamp() - $2::interval
  LIMIT $3::bigint
  FOR UPDATE SKIP LOCKED
)

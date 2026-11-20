WITH doomed AS (
  SELECT id FROM thread.runs
  WHERE status = ANY($1::text[]) AND updated_at < statement_timestamp() - $2::interval
  LIMIT $4::bigint
  FOR UPDATE SKIP LOCKED
), expired AS (
  UPDATE thread.runs AS l
  SET status = $3::text, updated_at = now()
  FROM doomed d
  WHERE l.id = d.id
  RETURNING l.runner_id
)
SELECT runner_id, count(*)::bigint AS expired_count
FROM expired
GROUP BY runner_id

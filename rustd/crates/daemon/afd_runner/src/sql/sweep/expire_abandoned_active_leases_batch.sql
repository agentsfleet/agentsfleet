WITH doomed AS (
  SELECT id, runner_id FROM runner.leases
  WHERE status = ANY($1::text[]) AND updated_at < statement_timestamp() - $2::interval
  LIMIT $4::bigint
  FOR UPDATE SKIP LOCKED
), tally AS (
  INSERT INTO runner.lifetime_counters
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT d.runner_id, 0, 0, 0, COUNT(*)::bigint, now(), now()
  FROM doomed d GROUP BY d.runner_id
  ON CONFLICT (runner_id) DO UPDATE
     SET expired_count = runner.lifetime_counters.expired_count + EXCLUDED.expired_count,
         updated_at = EXCLUDED.updated_at
)
UPDATE runner.leases AS l
SET status = $3::text, updated_at = now()
FROM doomed d
WHERE l.id = d.id

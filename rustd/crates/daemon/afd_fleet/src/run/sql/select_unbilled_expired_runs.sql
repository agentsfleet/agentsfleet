SELECT l.id AS run_id
FROM runner.runs l
-- Every owner's runs: a fleet's last slice is billed to the fleet, a draft's to its setup
-- work (`bill_expired_slice.sql`).
WHERE l.status <> $1
  AND l.run_expires_at < statement_timestamp() - $2::interval
  AND l.last_metered_at < l.run_expires_at
ORDER BY l.run_expires_at
LIMIT $3

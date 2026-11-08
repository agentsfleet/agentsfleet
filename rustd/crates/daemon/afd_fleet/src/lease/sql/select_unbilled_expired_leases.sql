SELECT l.id::text AS lease_id
FROM runner.leases l
WHERE l.status <> $1
  AND l.lease_expires_at < statement_timestamp() - $2::interval
  AND l.last_metered_at < l.lease_expires_at
ORDER BY l.lease_expires_at
LIMIT $3

SELECT l.id AS lease_id
FROM runner.leases l
-- Every owner's leases: a fleet's last slice is billed to the fleet, a draft's to its setup
-- work (`bill_expired_slice.sql`).
WHERE l.status <> $1
  AND l.lease_expires_at < statement_timestamp() - $2::interval
  AND l.last_metered_at < l.lease_expires_at
ORDER BY l.lease_expires_at
LIMIT $3

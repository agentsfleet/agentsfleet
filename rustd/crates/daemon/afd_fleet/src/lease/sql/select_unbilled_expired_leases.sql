SELECT l.id AS lease_id
FROM runner.leases l
-- A fleet's leases: their last slice is billed to the fleet (`bill_expired_slice.sql`).
WHERE l.status <> $1 AND l.fleet_id IS NOT NULL
  AND l.lease_expires_at < statement_timestamp() - $2::interval
  AND l.last_metered_at < l.lease_expires_at
ORDER BY l.lease_expires_at
LIMIT $3

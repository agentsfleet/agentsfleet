SELECT DISTINCT fleet_id::text AS fleet_id
FROM runner.leases
WHERE status = ANY($1::text[])
  AND lease_expires_at < statement_timestamp()
  AND fleet_id > $3::uuid
ORDER BY fleet_id
LIMIT $2::bigint

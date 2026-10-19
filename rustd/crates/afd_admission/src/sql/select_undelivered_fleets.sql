SELECT DISTINCT ON (fleet_id) fleet_id::text AS fleet_id, receipt
FROM fleet.admissions
WHERE receipt IS NOT NULL AND delivered_at IS NULL AND fleet_id > $2::uuid
ORDER BY fleet_id, created_at, sequence
LIMIT $1

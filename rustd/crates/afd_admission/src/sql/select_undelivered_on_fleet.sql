SELECT id::text AS id, receipt, created_at, sequence
FROM fleet.admissions
WHERE fleet_id = $1::uuid AND receipt IS NOT NULL AND delivered_at IS NULL
  AND (created_at, sequence) > ($3::timestamptz, $4::bigint)
ORDER BY created_at, sequence
LIMIT $2

SELECT id::text AS id, updated_at
FROM fleet.fleets
WHERE status = $1::text AND (updated_at, id) > ($2::timestamptz, $3::uuid)
ORDER BY updated_at ASC, id ASC
LIMIT $4::bigint

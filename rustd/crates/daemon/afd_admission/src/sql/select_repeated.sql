SELECT id::text AS id, payload_digest AS digest, fleet_id::text AS fleet, created_at
FROM fleet.admissions
WHERE producer = $1 AND producer_key = $2

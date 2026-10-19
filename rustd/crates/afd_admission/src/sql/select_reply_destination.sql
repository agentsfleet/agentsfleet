SELECT reply_provider, reply_address
FROM fleet.admissions
WHERE fleet_id = $1::uuid AND id = $2::uuid
  AND reply_provider IS NOT NULL

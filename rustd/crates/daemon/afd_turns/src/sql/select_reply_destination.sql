SELECT reply_provider, reply_address
FROM fleet.admissions
WHERE fleet_id = $1 AND id = $2
  AND reply_provider IS NOT NULL

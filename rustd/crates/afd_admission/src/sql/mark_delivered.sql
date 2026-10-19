UPDATE fleet.admissions
SET delivered_at = now(), updated_at = now()
WHERE fleet_id = $1::uuid AND id = $2::uuid
  AND delivered_at IS NULL

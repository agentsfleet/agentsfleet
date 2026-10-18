UPDATE fleet.admissions
SET receipt = NULL, updated_at = now()
WHERE id = $1::uuid AND receipt = $2::text AND delivered_at IS NULL

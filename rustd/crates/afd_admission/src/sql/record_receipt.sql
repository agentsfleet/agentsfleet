UPDATE fleet.admissions
SET receipt = $2, updated_at = now()
WHERE id = $1::uuid AND receipt IS NULL

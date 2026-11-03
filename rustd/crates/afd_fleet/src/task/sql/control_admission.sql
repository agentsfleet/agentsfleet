UPDATE fleet.admissions SET task_id = $1::uuid, updated_at = now()
WHERE id = $2::uuid AND task_id IS NULL

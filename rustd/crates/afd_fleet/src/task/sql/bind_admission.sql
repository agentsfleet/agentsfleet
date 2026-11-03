UPDATE fleet.admissions SET task_id = $1::uuid, task_item_id = $2::uuid, updated_at = now()
WHERE id = $3::uuid AND task_id IS NULL

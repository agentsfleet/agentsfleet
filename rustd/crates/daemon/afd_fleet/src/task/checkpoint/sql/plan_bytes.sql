UPDATE fleet.task_checkpoints SET uploaded_bytes = uploaded_bytes + $2, updated_at = now()
WHERE id = $1::uuid

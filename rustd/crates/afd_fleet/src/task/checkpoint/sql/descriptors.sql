UPDATE fleet.task_objects o
SET ciphertext_digest = n.digest, ciphertext_length = n.length, updated_at = now()
FROM jsonb_to_recordset($1::jsonb) AS n(id uuid, digest text, length bigint)
WHERE o.id = n.id AND o.workspace_id = $2::uuid AND o.ciphertext_digest IS NULL

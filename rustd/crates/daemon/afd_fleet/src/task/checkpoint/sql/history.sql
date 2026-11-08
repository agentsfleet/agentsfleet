SELECT o.id, o.plaintext_digest, o.plaintext_size
FROM unnest($1::text[]) WITH ORDINALITY n(id, position)
JOIN fleet.task_objects o ON o.id = n.id::uuid AND o.workspace_id = $2
WHERE o.verified_at IS NOT NULL AND o.tombstoned_at IS NULL
ORDER BY n.position

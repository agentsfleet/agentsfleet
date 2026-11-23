SELECT id FROM thread.turns
WHERE id = $1 AND thread_id = $2 AND workspace_id = $3 AND interpreted_at IS NOT NULL
FOR UPDATE

SELECT id FROM thread.turns WHERE thread_id = $1 AND settled_at IS NULL ORDER BY created_at, sequence LIMIT 1

SELECT count(*) FROM thread.turns WHERE thread_id = ANY($1) AND settled_at IS NULL

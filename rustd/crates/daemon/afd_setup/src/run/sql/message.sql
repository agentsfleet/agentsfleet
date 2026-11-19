SELECT EXISTS (
    SELECT 1 FROM thread.turns
     WHERE thread_id = $1 AND sequence = $2 AND status = $3
)

SELECT EXISTS (
    SELECT 1 FROM thread.turns
     WHERE thread_id = $1 AND status IN ($2, $3)
)

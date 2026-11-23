UPDATE thread.turns SET save_error_code = $3, updated_at = now()
 WHERE thread_id = $1 AND sequence = $2

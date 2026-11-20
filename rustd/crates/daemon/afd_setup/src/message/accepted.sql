SELECT save_requested FROM thread.turns
 WHERE thread_id = $1 AND sequence = $2

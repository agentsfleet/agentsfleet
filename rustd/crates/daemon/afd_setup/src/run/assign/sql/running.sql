UPDATE thread.turns
   SET status = $3, updated_at = now()
 WHERE thread_id = $1 AND sequence = $2 AND status = $4

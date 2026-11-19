UPDATE thread.turns
   SET status = $3, failure_reason = $4, settled_at = now(), updated_at = now()
 WHERE thread_id = $1 AND sequence = $2 AND status = $5

UPDATE thread.turns
   SET status = $3, answer_text = NULL, settled_at = now(), updated_at = now()
 WHERE thread_id = $1 AND sequence <= $2 AND status IN ($4, $5)
RETURNING sequence

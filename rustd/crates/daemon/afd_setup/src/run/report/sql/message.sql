UPDATE thread.turns
   SET status = $3, answer_text = $4, candidate_id = $5, failure_reason = $6,
       settled_at = now(), updated_at = now()
 WHERE thread_id = $1 AND sequence = $2 AND status = $7

UPDATE authoring.messages
   SET status = $3, answer_text = NULL, finished_at = now(), updated_at = now()
 WHERE draft_id = $1::uuid AND sequence <= $2 AND status IN ($4, $5)
RETURNING sequence

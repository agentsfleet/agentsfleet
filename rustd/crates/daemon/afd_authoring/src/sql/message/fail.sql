UPDATE authoring.messages
   SET status = $3, failure_reason = $4, finished_at = now(), updated_at = now()
 WHERE draft_id = $1::uuid AND sequence = $2 AND status = $5

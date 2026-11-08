UPDATE authoring.messages
   SET status = $3, answer_text = $4, candidate_id = $5::uuid, failure_reason = $6,
       finished_at = now(), updated_at = now()
 WHERE draft_id = $1::uuid AND sequence = $2 AND status = $7

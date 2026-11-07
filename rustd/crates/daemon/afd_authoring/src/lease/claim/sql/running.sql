UPDATE authoring.messages
   SET status = $3, updated_at = now()
 WHERE draft_id = $1::uuid AND sequence = $2 AND status = $4

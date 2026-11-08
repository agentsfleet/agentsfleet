UPDATE authoring.messages
   SET failure_reason = $3, updated_at = now()
 WHERE draft_id = $1 AND sequence = $2 AND failure_reason = $4

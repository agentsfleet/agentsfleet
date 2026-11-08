UPDATE authoring.attempts
   SET last_activity_sequence = $2, updated_at = now()
 WHERE id = $1::uuid

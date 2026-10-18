UPDATE authoring.attempts
   SET failure_reason = COALESCE(failure_reason, $2), updated_at = now()
 WHERE id = $1::uuid

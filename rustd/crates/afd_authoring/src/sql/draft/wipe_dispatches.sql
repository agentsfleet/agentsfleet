UPDATE authoring.dispatches
   SET result = NULL, private_expires_at = now(), updated_at = now()
 WHERE attempt_id IN (SELECT id FROM authoring.attempts WHERE draft_id = $1::uuid)

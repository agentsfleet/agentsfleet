UPDATE setup.dispatches
   SET result = NULL, native_reply = NULL, private_expires_at = now(), updated_at = now()
 WHERE attempt_id IN (SELECT id FROM setup.attempts WHERE draft_id = $1)

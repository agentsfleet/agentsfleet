UPDATE authoring.dispatches
   SET status = $3, result = $4::jsonb, usage = $5::jsonb, native_reply = $8::jsonb,
       failure_cause = $9, updated_at = now()
 WHERE id = $1 AND attempt_id = $2 AND status IN ($6, $7)

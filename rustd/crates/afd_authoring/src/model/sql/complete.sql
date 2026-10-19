UPDATE authoring.dispatches
   SET status = $3, result = $4::jsonb, usage = $5::jsonb, native_reply = $8::jsonb,
       updated_at = now()
 WHERE id = $1::uuid AND attempt_id = $2::uuid AND status IN ($6, $7)

UPDATE fleet.obligations
   SET receipt = $2::text, updated_at = now()
 WHERE id = $1::uuid

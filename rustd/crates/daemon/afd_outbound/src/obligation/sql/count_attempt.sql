UPDATE fleet.obligations
   SET attempt_count = attempt_count + 1,
       updated_at    = now()
 WHERE id = $1 AND receipt = $2::text AND delivered_at IS NULL
RETURNING attempt_count

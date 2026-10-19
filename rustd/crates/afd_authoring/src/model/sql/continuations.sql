SELECT native_reply::text AS native_reply,
       private_expires_at
  FROM authoring.dispatches
 WHERE attempt_id = $1::uuid AND sequence < $2 AND status = $3
   AND native_reply IS NOT NULL
 ORDER BY sequence
 LIMIT $4

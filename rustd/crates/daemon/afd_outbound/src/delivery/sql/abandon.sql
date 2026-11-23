UPDATE thread.deliveries
   SET abandoned_at = now(), updated_at = now()
 WHERE id = $1 AND receipt = $2
   AND delivered_at IS NULL AND abandoned_at IS NULL
RETURNING attempt_count

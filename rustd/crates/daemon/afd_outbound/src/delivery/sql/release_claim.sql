UPDATE thread.deliveries
   SET claim_expires_at = clock_timestamp(),
       updated_at       = now()
 WHERE id = $1 AND receipt = $2
   AND delivered_at IS NULL AND abandoned_at IS NULL

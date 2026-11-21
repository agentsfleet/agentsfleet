UPDATE thread.deliveries
   SET delivered_at = now(),
       updated_at   = now()
 WHERE id = $1 AND receipt = $2 AND delivered_at IS NULL

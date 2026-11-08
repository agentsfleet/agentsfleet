UPDATE fleet.obligations
   SET delivered_at = now(),
       updated_at   = now()
 WHERE id = $1 AND receipt = $2::text AND delivered_at IS NULL

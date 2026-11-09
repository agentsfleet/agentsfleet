UPDATE fleet.obligations
   SET attempt_count    = attempt_count + 1,
       claim_expires_at = clock_timestamp() + $3::interval,
       updated_at       = now()
 WHERE id = $1 AND receipt = $2 AND delivered_at IS NULL
RETURNING attempt_count

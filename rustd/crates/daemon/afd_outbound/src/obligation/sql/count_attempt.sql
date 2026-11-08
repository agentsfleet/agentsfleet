UPDATE fleet.obligations
   SET attempt_count = attempt_count + 1,
       updated_at    = now()
 WHERE fleet_id = $1 AND event_id = $2 AND delivered_at IS NULL
RETURNING attempt_count

UPDATE fleet.obligations
   SET delivered_at = now(),
       updated_at   = now()
 WHERE fleet_id = $1 AND event_id = $2 AND delivered_at IS NULL

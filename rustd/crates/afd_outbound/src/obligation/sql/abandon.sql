UPDATE fleet.obligations
   SET abandoned_at = now(), abandon_reason = $3::text, updated_at = now()
 WHERE fleet_id = $1::uuid AND event_id = $2::text
   AND delivered_at IS NULL AND abandoned_at IS NULL
RETURNING attempt_count

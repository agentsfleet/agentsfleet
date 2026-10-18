SELECT id::text,
       (recipe->>'slot')::bigint AS slot,
       recipe::text,
       sync_token IS NOT NULL
           AND coalesce(sync_lease_expires_at > clock_timestamp(), false) AS held
  FROM fleet.schedules
 WHERE fleet_id = $1::uuid AND recipe IS NOT NULL
   FOR UPDATE

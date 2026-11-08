WITH locked AS MATERIALIZED (
    SELECT id, recipe, desired_status, sync_token, sync_lease_expires_at
      FROM fleet.schedules
     WHERE fleet_id = $1 AND recipe IS NOT NULL
       FOR UPDATE
)
SELECT id,
       (recipe->>'slot')::bigint AS slot,
       recipe::text,
       desired_status,
       sync_token IS NOT NULL
           AND coalesce(sync_lease_expires_at > clock_timestamp(), false) AS held
  FROM locked

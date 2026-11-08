UPDATE fleet.schedules
   SET sync_status = $4,
       sync_token = NULL,
       sync_lease_expires_at = NULL,
       last_error = $5,
       updated_at = now()
 WHERE id = $1 AND resource_version = $2 AND sync_token = $3
RETURNING
       id, fleet_id, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text

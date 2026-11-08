UPDATE fleet.schedules
   SET desired_status = $2,
       sync_status = $3,
       resource_version = resource_version + 1,
       sync_token = NULL,
       sync_lease_expires_at = NULL,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1::uuid
RETURNING
       id::text, fleet_id::text, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token::text, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text

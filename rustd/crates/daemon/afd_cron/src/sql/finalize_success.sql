UPDATE fleet.schedules
   SET sync_status = $4,
       sync_token = NULL,
       sync_lease_expires_at = NULL,
       last_error = NULL,
       updated_at = now(),
       provider_schedule_id = COALESCE($5, provider_schedule_id)
 WHERE id = $1::uuid AND resource_version = $2 AND sync_token = $3::uuid
RETURNING
       id::text, fleet_id::text, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token::text, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text

UPDATE fleet.schedules
   SET fire_at = $3
 WHERE id = $1 AND sync_token = $2
RETURNING
       id, fleet_id, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text

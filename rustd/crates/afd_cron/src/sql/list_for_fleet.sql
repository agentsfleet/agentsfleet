SELECT
       id::text, fleet_id::text, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token::text, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text
  FROM fleet.schedules
 WHERE fleet_id = $1::uuid
 ORDER BY created_at, id

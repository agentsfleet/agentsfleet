UPDATE fleet.schedules
   SET sync_status = $3,
       resource_version = resource_version + 1,
       sync_token = $4,
       sync_lease_expires_at = clock_timestamp() + $5::interval,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1 AND fleet_id = $2
   AND (sync_token IS NULL OR sync_lease_expires_at IS NULL
        OR sync_lease_expires_at <= clock_timestamp())
RETURNING
       id, fleet_id, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text

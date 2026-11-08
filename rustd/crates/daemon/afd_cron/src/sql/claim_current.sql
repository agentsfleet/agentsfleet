UPDATE fleet.schedules
   SET sync_status = $3,
       resource_version = resource_version + 1,
       sync_token = $4::uuid,
       sync_lease_expires_at = clock_timestamp() + $5::interval,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1::uuid AND fleet_id = $2::uuid
   AND (sync_token IS NULL OR sync_lease_expires_at IS NULL
        OR sync_lease_expires_at <= clock_timestamp())
RETURNING
       id::text, fleet_id::text, source, provider_schedule_id, cron_expression,
       timezone, message, once, desired_status, sync_status, resource_version,
       sync_token::text, sync_lease_expires_at, last_error, created_at, updated_at,
       fire_at, recipe::text

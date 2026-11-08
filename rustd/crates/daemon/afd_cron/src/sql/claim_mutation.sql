UPDATE fleet.schedules
   SET cron_expression = COALESCE($3, cron_expression),
       timezone = COALESCE($4, timezone),
       message = COALESCE($5, message),
       desired_status = COALESCE($6, desired_status),
       sync_status = $7,
       resource_version = resource_version + 1,
       sync_token = $8::uuid,
       sync_lease_expires_at = clock_timestamp() + $9::interval,
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

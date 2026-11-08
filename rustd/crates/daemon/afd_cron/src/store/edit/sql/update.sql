UPDATE fleet.schedules
   SET cron_expression = $2,
       timezone = $3,
       message = $4,
       desired_status = $5,
       recipe = $6::jsonb,
       sync_status = $7,
       resource_version = resource_version + 1,
       sync_token = $8::uuid,
       sync_lease_expires_at = clock_timestamp() + $9::interval,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1::uuid

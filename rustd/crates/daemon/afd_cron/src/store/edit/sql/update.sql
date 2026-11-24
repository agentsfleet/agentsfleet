UPDATE fleet.schedules
   SET cron_expression = $2,
       timezone = $3,
       message = $4,
       desired_status = $5,
       sync_status = $6,
       resource_version = resource_version + 1,
       sync_token = $7,
       sync_run_expires_at = clock_timestamp() + $8::interval,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1

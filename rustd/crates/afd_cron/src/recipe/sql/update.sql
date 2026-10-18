UPDATE fleet.schedules
   SET cron_expression = $2,
       timezone = $3,
       message = $4,
       desired_status = $5,
       sync_status = $6,
       resource_version = resource_version + 1,
       recipe = $7::jsonb,
       sync_token = NULL,
       sync_lease_expires_at = NULL,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1::uuid

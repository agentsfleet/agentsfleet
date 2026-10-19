UPDATE fleet.schedules
   SET desired_status = $2,
       sync_status = $3,
       resource_version = resource_version + 1,
       sync_token = NULL,
       sync_lease_expires_at = NULL,
       last_error = NULL,
       updated_at = now()
 WHERE id = $1::uuid

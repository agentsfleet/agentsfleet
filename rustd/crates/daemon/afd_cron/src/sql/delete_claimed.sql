DELETE FROM fleet.schedules
 WHERE id = $1::uuid AND resource_version = $2 AND sync_token = $3::uuid
RETURNING id::text

DELETE FROM fleet.schedules
 WHERE id = $1 AND resource_version = $2 AND sync_token = $3
RETURNING id

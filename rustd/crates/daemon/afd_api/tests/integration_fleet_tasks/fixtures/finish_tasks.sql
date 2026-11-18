UPDATE fleet.tasks
   SET status = $2, terminal_at = now(), updated_at = now(),
       resource_version = resource_version + 1
 WHERE fleet_id = $1 AND terminal_at IS NULL

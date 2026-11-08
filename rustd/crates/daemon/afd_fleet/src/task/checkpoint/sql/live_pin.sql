SELECT id, kind, owner_lease_id, owner_fence, task_generation,
       stop_generation, resource_version, expires_at, operation_expires_at, released_at IS NOT NULL AS released
FROM fleet.task_object_pins WHERE task_checkpoint_id = $1 AND kind = $2 AND released_at IS NULL
FOR UPDATE

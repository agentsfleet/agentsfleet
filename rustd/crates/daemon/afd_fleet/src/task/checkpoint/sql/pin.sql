SELECT id, kind, owner_lease_id, owner_fence, task_generation,
       stop_generation, resource_version, expires_at, operation_expires_at,
       released_at IS NOT NULL AS released
FROM fleet.task_object_pins
WHERE id = $1 AND task_checkpoint_id = $2
FOR UPDATE

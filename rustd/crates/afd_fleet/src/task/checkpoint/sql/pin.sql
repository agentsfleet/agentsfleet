SELECT id::text, kind, owner_lease_id::text, owner_fence, task_generation,
       stop_generation, resource_version, expires_at, operation_expires_at,
       released_at IS NOT NULL AS released
FROM fleet.task_object_pins
WHERE id = $1::uuid AND task_checkpoint_id = $2::uuid
FOR UPDATE

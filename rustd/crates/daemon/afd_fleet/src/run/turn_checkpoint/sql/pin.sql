SELECT id, owner_run_id, owner_fence, kind, operation_id, resource_version, expires_at,
       operation_expires_at, released_at IS NOT NULL AS released
FROM runner.checkpoint_pins
WHERE id = $1 AND checkpoint_id = $2 AND operation_id IS NOT NULL
FOR UPDATE

SELECT id, owner_run_id, owner_fence, kind, operation_id, resource_version, expires_at,
       operation_expires_at, released_at IS NOT NULL AS released
FROM runner.turn_checkpoint_pins
WHERE id = $1 AND turn_checkpoint_id = $2
FOR UPDATE

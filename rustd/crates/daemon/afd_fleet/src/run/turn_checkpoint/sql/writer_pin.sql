SELECT id, owner_run_id, owner_fence, kind, operation_id, resource_version, expires_at,
       operation_expires_at, released_at IS NOT NULL AS released
FROM runner.turn_checkpoint_pins
WHERE turn_checkpoint_id = $1 AND kind = $2
ORDER BY created_at, id
LIMIT 1
FOR UPDATE

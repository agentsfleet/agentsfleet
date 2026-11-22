SELECT id, owner_run_id, owner_fence, kind, operation_id, resource_version, expires_at,
       operation_expires_at, released_at IS NOT NULL AS released
FROM runner.checkpoint_pins
WHERE checkpoint_id = $1 AND kind = $2 AND operation_id IS NOT NULL
ORDER BY created_at, id
LIMIT 1
FOR UPDATE

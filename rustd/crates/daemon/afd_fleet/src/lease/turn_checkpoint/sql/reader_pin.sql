SELECT id, owner_lease_id, owner_fence, kind, operation_id, resource_version, expires_at,
       operation_expires_at, released_at IS NOT NULL AS released
FROM runner.turn_checkpoint_pins
WHERE turn_checkpoint_id = $1 AND owner_lease_id = $2 AND owner_fence = $3
  AND kind = $4 AND (operation_id = $5 OR released_at IS NULL)
ORDER BY operation_id = $5 DESC, created_at DESC
LIMIT 1
FOR UPDATE

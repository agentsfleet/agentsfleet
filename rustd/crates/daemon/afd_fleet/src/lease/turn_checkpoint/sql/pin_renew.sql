UPDATE runner.turn_checkpoint_pins
SET expires_at = LEAST(operation_expires_at, $4 + make_interval(secs => $5)),
    resource_version = resource_version + 1, updated_at = $4
WHERE id = $1 AND resource_version = $2 AND resource_version < $3
  AND released_at IS NULL AND expires_at > $4 AND operation_expires_at > $4
RETURNING id, owner_lease_id, owner_fence, kind, operation_id, resource_version,
          expires_at, operation_expires_at, released_at IS NOT NULL AS released

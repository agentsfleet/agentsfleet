INSERT INTO runner.turn_checkpoint_pins
    (id, turn_checkpoint_id, kind, owner_lease_id, owner_fence, resource_version, operation_id,
     expires_at, operation_expires_at, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, 1, $8,
        LEAST($6, clock_timestamp() + make_interval(secs => $7)), $6,
        clock_timestamp(), clock_timestamp())
RETURNING id, owner_lease_id, owner_fence, kind, operation_id, resource_version,
          expires_at, operation_expires_at, released_at IS NOT NULL AS released

SELECT id, kind, owner_run_id, owner_fence, durable_task_generation,
       stop_generation, resource_version, expires_at, operation_expires_at, released_at IS NOT NULL AS released
FROM runner.checkpoint_pins
WHERE checkpoint_id = $1 AND kind = $2 AND released_at IS NULL AND durable_task_generation IS NOT NULL
FOR UPDATE

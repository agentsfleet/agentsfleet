UPDATE fleet.task_object_pins
SET resource_version = resource_version + 1,
    expires_at = LEAST(operation_expires_at, clock_timestamp() + make_interval(secs => $2)), updated_at = now()
WHERE id = $1

WITH held AS MATERIALIZED (
    SELECT status AS state, resource_version, generation, expires_at,
           max_model_nanos, used_model_nanos, reserved_model_nanos,
           max_running_nanos, used_running_nanos, max_leases, used_leases
    FROM fleet.tasks
    WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
    FOR UPDATE
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM held
)
SELECT held.state, held.resource_version, held.generation,
       held.expires_at > instant.checked_at
       AND held.used_model_nanos < held.max_model_nanos - held.reserved_model_nanos
       AND held.used_running_nanos < held.max_running_nanos
       AND held.used_leases < held.max_leases AS can_resume
FROM held CROSS JOIN instant

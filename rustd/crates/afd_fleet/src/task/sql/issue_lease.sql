WITH instant AS MATERIALIZED (SELECT clock_timestamp() AS at), started AS (
    UPDATE fleet.tasks t SET status = $4, used_leases = used_leases + 1,
        resource_version = resource_version + 1, updated_at = now()
    FROM runner.leases l CROSS JOIN instant
    WHERE t.id = $2::uuid AND l.id = $1::uuid AND l.fleet_id = t.fleet_id AND l.workspace_id = t.workspace_id
      AND t.generation = $5 AND t.status IN ($3, $4) AND t.terminal_at IS NULL
      AND t.expires_at > instant.at AND t.used_leases < t.max_leases
      AND t.used_model_nanos < t.max_model_nanos - t.reserved_model_nanos
      AND t.used_running_nanos < t.max_running_nanos
    RETURNING t.id, t.generation, t.stop_generation,
        LEAST(t.expires_at, instant.at + make_interval(secs => LEAST($6::double precision,
            (t.max_running_nanos - t.used_running_nanos)::double precision / $7::bigint))) AS deadline,
        instant.at
), bound AS (
    UPDATE runner.leases l SET task_id = s.id, task_generation = s.generation,
        task_stop_generation = s.stop_generation, task_started_at = s.at, task_runtime_accounted_at = s.at,
        task_execution_deadline_at = s.deadline, lease_expires_at = LEAST(l.lease_expires_at, s.deadline), updated_at = now()
    FROM started s WHERE l.id = $1::uuid
    RETURNING l.fleet_id, l.fencing_token, l.lease_expires_at
)
UPDATE runner.fleet_slots s SET lease_expires_at = b.lease_expires_at, updated_at = now()
FROM bound b WHERE s.fleet_id = b.fleet_id AND s.fencing_token = b.fencing_token
RETURNING s.lease_expires_at

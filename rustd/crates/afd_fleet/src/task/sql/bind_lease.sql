WITH instant AS MATERIALIZED (SELECT clock_timestamp() AS at), timed AS MATERIALIZED (
    SELECT t.id, t.fleet_id, t.workspace_id, t.generation, t.stop_generation, instant.at,
        LEAST(t.expires_at, instant.at + make_interval(secs => LEAST($6::double precision,
            (t.max_running_nanos - t.used_running_nanos)::double precision / $7::bigint))) AS deadline
    FROM fleet.tasks t CROSS JOIN instant WHERE t.id = $1::uuid
), bound AS (
    UPDATE runner.leases l SET task_id = t.id, task_generation = t.generation,
        task_stop_generation = t.stop_generation, task_started_at = t.at,
        task_runtime_accounted_at = t.at, task_execution_deadline = t.deadline,
        lease_expires_at = LEAST(l.lease_expires_at, t.deadline), updated_at = now()
    FROM timed t
    WHERE l.id = $2::uuid AND l.runner_id = $3::uuid
      AND l.task_id IS NULL AND l.fleet_id = t.fleet_id AND l.workspace_id = t.workspace_id
    RETURNING l.id, t.id AS task_id, l.fleet_id, l.fencing_token, l.lease_expires_at
), slot AS (
    UPDATE runner.fleet_slots s SET lease_expires_at = b.lease_expires_at, updated_at = now()
    FROM bound b WHERE s.fleet_id = b.fleet_id AND s.fencing_token = b.fencing_token
    RETURNING s.fleet_id
)
UPDATE fleet.tasks t SET used_model_nanos = t.used_model_nanos + (
    SELECT COALESCE(sum(u.charged_nanos), 0) FROM billing.usage_lines u
    WHERE u.work_type = $4 AND u.work_id = $2::uuid AND u.seq < $5), updated_at = now()
FROM bound WHERE t.id = bound.task_id AND EXISTS (SELECT 1 FROM slot)

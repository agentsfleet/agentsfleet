WITH instant AS MATERIALIZED (SELECT clock_timestamp() AS at), timed AS MATERIALIZED (
    SELECT t.id, t.fleet_id, t.workspace_id, t.generation, t.stop_generation, instant.at,
        LEAST(t.expires_at, instant.at + make_interval(secs => LEAST($6::double precision,
            (t.max_running_nanos - t.used_running_nanos)::double precision / $7::bigint))) AS deadline
    FROM fleet.durable_tasks t CROSS JOIN instant WHERE t.id = $1
), bound AS (
    UPDATE thread.runs l SET durable_task_id = t.id, durable_task_generation = t.generation,
        durable_task_stop_generation = t.stop_generation, durable_task_started_at = t.at,
        durable_task_runtime_accounted_at = t.at, durable_task_execution_deadline_at = t.deadline,
        run_expires_at = LEAST(l.run_expires_at, t.deadline), updated_at = now()
    FROM timed t
    WHERE l.id = $2 AND l.runner_id = $3
      AND l.durable_task_id IS NULL AND l.fleet_id = t.fleet_id AND l.workspace_id = t.workspace_id
    RETURNING l.id, t.id AS durable_task_id, l.fleet_id, l.fencing_token, l.run_expires_at
), moved_thread AS (
    UPDATE thread.threads s SET run_expires_at = b.run_expires_at, updated_at = now()
    FROM bound b WHERE s.fleet_id = b.fleet_id AND s.fencing_token = b.fencing_token
    RETURNING s.fleet_id
)
UPDATE fleet.durable_tasks t SET used_model_nanos = t.used_model_nanos + (
    SELECT COALESCE(sum(u.charged_nanos), 0) FROM billing.usage_lines u
    WHERE u.work_type = $4 AND u.work_id = $2 AND u.kind = $5), updated_at = now()
FROM bound WHERE t.id = bound.durable_task_id AND EXISTS (SELECT 1 FROM moved_thread)

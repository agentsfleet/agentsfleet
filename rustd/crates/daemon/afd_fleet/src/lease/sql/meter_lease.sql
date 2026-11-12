WITH locked_slot AS MATERIALIZED (
    SELECT s.fleet_id, s.fencing_token, s.lease_expires_at
    FROM runner.fleet_slots s JOIN runner.leases l ON l.fleet_id = s.fleet_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR UPDATE OF s
), probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id,
           l.fencing_token, l.created_at, l.lease_expires_at, l.last_metered_at,
           a.fencing_token AS live_fence, a.lease_expires_at AS slot_expires_at,
           l.task_id, l.task_generation, l.task_stop_generation, l.task_execution_deadline_at,
           l.task_runtime_accounted_at
    FROM runner.leases l
    JOIN locked_slot a ON a.fleet_id = l.fleet_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR UPDATE OF l
), held_task AS MATERIALIZED (
    SELECT t.id, t.status, t.generation, t.stop_generation, t.expires_at,
           t.max_model_nanos, t.used_model_nanos, t.reserved_model_nanos,
           t.max_running_nanos, t.used_running_nanos
    FROM fleet.tasks t JOIN probe p ON p.task_id = t.id
      AND t.fleet_id = p.fleet_id AND t.workspace_id = p.workspace_id
    FOR UPDATE OF t
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM probe CROSS JOIN (SELECT count(*) FROM held_task) held
), metered AS (
    -- The run time since the lease's cursor, never past the deadline it was
    -- granted: a report that lands after its lease lapsed bills no time beyond it.
    SELECT p.*, i.checked_at, t.status AS task_status, t.generation AS live_task_generation,
           t.stop_generation AS live_stop_generation, t.expires_at AS task_expires_at,
           t.used_model_nanos, t.max_model_nanos, t.reserved_model_nanos,
           t.max_running_nanos - t.used_running_nanos AS remaining_running_nanos,
           GREATEST(p.last_metered_at, LEAST(i.checked_at, p.lease_expires_at)) AS metered_until
    FROM probe p CROSS JOIN instant i LEFT JOIN held_task t ON t.id = p.task_id
), guard AS (
    -- A renewal must still be the live holder, before either deadline and
    -- below the hard cap.
    SELECT *, LEAST(checked_at + $5::interval, created_at + $6::interval, task_execution_deadline_at,
                    task_expires_at, task_runtime_accounted_at + make_interval(secs => remaining_running_nanos::double precision / 1000000000)) AS capped,
           (EXTRACT(EPOCH FROM metered_until - last_metered_at) * $3::bigint)::bigint AS d_ms
    FROM metered
    WHERE fencing_token = live_fence
      AND LEAST(checked_at + $5::interval, created_at + $6::interval) > checked_at
      AND lease_expires_at > checked_at AND slot_expires_at > checked_at
      AND (task_id IS NULL OR (
          task_status = $7 AND task_generation = live_task_generation
          AND task_stop_generation = live_stop_generation AND task_expires_at > checked_at
          AND used_model_nanos < max_model_nanos - reserved_model_nanos
          AND task_execution_deadline_at > checked_at
          AND task_runtime_accounted_at + make_interval(secs => remaining_running_nanos::double precision / 1000000000) > checked_at))
      AND EXISTS (
          SELECT 1 FROM fleet.admissions a
          WHERE a.id = metered.event_id AND a.fleet_id = metered.fleet_id
            AND a.lease_fence = metered.fencing_token AND a.settled_at IS NULL
      )
), moved AS (
    UPDATE runner.leases l
    SET lease_expires_at = g.capped,
        last_metered_at  = g.metered_until,
        updated_at = now()
    FROM guard g WHERE l.id = g.id
    RETURNING l.lease_expires_at AS capped
), slot AS (
    UPDATE runner.fleet_slots a
    SET lease_expires_at = g.capped,
        updated_at = now()
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
)
SELECT
    (SELECT count(*) FROM probe)::bigint          AS probe_found,
    (SELECT count(*) FROM moved)::bigint          AS claimed,
    (SELECT capped FROM moved)                    AS new_until,
    (SELECT LEAST(created_at + $6::interval, task_execution_deadline_at) FROM probe) AS hard_cap,
    g.metered_until AS checked_at, g.created_at AS started_at, g.d_ms,
    g.tenant_id AS tenant_id, g.workspace_id AS workspace_id,
    g.fleet_id AS fleet_id, g.event_id AS event_id,
    (SELECT f.name FROM fleet.fleets f WHERE f.id = g.fleet_id) AS fleet_name,
    -- The runtime line's number: the milliseconds from the lease's start to the
    -- slice's. The cursor only moves forward under the lease's row lock, so no
    -- two slices share a start, and the same slice billed twice is one line.
    (EXTRACT(EPOCH FROM g.last_metered_at - g.created_at) * $3::bigint)::bigint AS runtime_seq
FROM (SELECT 1) AS one
LEFT JOIN guard g ON true

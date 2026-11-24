-- The fleet's per-run renewal: one run and its thread extended by one lifetime and metered,
-- with the durable task's limits and the turn's fence checked in the same statement. A change
-- to this bookkeeping belongs in `renew_batch.sql` too.
WITH locked_thread AS MATERIALIZED (
    SELECT s.fleet_id, s.fencing_token, s.run_expires_at
    FROM thread.threads s JOIN thread.runs l ON l.fleet_id = s.fleet_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR NO KEY UPDATE OF s
), probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.turn_id,
           l.fencing_token, l.created_at, l.run_expires_at, l.last_metered_at,
           a.fencing_token AS live_fence, a.run_expires_at AS thread_expires_at,
           l.durable_task_id, l.durable_task_generation, l.durable_task_stop_generation, l.durable_task_execution_deadline_at,
           l.durable_task_runtime_accounted_at
    FROM thread.runs l
    JOIN locked_thread a ON a.fleet_id = l.fleet_id
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR NO KEY UPDATE OF l
), held_durable_task AS MATERIALIZED (
    SELECT t.id, t.status, t.generation, t.stop_generation, t.expires_at,
           t.max_model_nanos, t.used_model_nanos,
           t.max_running_nanos, t.used_running_nanos
    FROM fleet.durable_tasks t JOIN probe p ON p.durable_task_id = t.id
      AND t.fleet_id = p.fleet_id AND t.workspace_id = p.workspace_id
    FOR UPDATE OF t
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM probe CROSS JOIN (SELECT count(*) FROM held_durable_task) held
), metered AS (
    -- The run time since the run's cursor, never past the deadline it was
    -- granted: a report that lands after its run lapsed bills no time beyond it.
    SELECT p.*, i.checked_at, t.status AS durable_task_status, t.generation AS live_durable_task_generation,
           t.stop_generation AS live_stop_generation, t.expires_at AS durable_task_expires_at,
           t.used_model_nanos, t.max_model_nanos,
           t.max_running_nanos - t.used_running_nanos AS remaining_running_nanos,
           GREATEST(p.last_metered_at, LEAST(i.checked_at, p.run_expires_at)) AS metered_until
    FROM probe p CROSS JOIN instant i LEFT JOIN held_durable_task t ON t.id = p.durable_task_id
), guard AS (
    -- A renewal must still be the live holder, before either deadline. A run
    -- grants one more lifetime at any age; only a durable task's own limits
    -- shorten it.
    SELECT *, LEAST(checked_at + $5::interval, durable_task_execution_deadline_at,
                    durable_task_expires_at, durable_task_runtime_accounted_at + make_interval(secs => remaining_running_nanos::double precision / 1000000000)) AS capped,
           (EXTRACT(EPOCH FROM metered_until - last_metered_at) * $3::bigint)::bigint AS d_ms
    FROM metered
    WHERE fencing_token = live_fence
      AND run_expires_at > checked_at AND thread_expires_at > checked_at
      AND (durable_task_id IS NULL OR (
          durable_task_status = $6 AND durable_task_generation = live_durable_task_generation
          AND durable_task_stop_generation = live_stop_generation AND durable_task_expires_at > checked_at
          AND used_model_nanos < max_model_nanos
          AND durable_task_execution_deadline_at > checked_at
          AND durable_task_runtime_accounted_at + make_interval(secs => remaining_running_nanos::double precision / 1000000000) > checked_at))
      AND EXISTS (
          SELECT 1 FROM thread.turns a
          WHERE a.id = metered.turn_id AND a.thread_id = metered.fleet_id
            AND a.run_fence = metered.fencing_token AND a.settled_at IS NULL
      )
), moved AS (
    UPDATE thread.runs l
    SET run_expires_at = g.capped,
        last_metered_at  = g.metered_until,
        updated_at = now()
    FROM guard g WHERE l.id = g.id
    RETURNING l.run_expires_at AS capped
), moved_thread AS (
    UPDATE thread.threads a
    SET run_expires_at = g.capped,
        updated_at = now()
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
)
SELECT
    (SELECT count(*) FROM moved)::bigint          AS moved_runs,
    (SELECT capped FROM moved)                    AS new_until,
    g.metered_until AS checked_at, g.created_at AS started_at, g.d_ms,
    g.tenant_id AS tenant_id, g.workspace_id AS workspace_id,
    g.fleet_id AS fleet_id, g.turn_id AS turn_id,
    (SELECT f.name FROM fleet.fleets f WHERE f.id = g.fleet_id) AS fleet_name,
    -- The runtime line's number: the milliseconds from the run's start to the
    -- slice's. The cursor only moves forward under the run's row lock, so no
    -- two slices share a start, and the same slice billed twice is one line.
    (EXTRACT(EPOCH FROM g.last_metered_at - g.created_at) * $3::bigint)::bigint AS runtime_seq
FROM (SELECT 1) AS one
LEFT JOIN guard g ON true

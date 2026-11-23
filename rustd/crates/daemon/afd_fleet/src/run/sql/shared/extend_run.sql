-- The renewal both kinds share: the run and its thread moved to one more run lifetime under
-- the run's fence, and the runtime since the run's cursor measured, in one statement.
-- Both rows move or neither does. The caller holds the host row and has run
-- `lock_cancelled.sql`, which takes the thread and run locks in this order and refuses a
-- cancelled run; the owner's own liveness check and the slice's usage line are its own
-- statements, in the same transaction.
--
-- $1 run · $2 runner · $3 milliseconds per second · $4 active status · $5 run TTL
WITH locked_thread AS MATERIALIZED (
    SELECT s.id, s.fencing_token, s.run_expires_at
    FROM thread.threads s JOIN thread.runs l ON s.id = COALESCE(l.fleet_id, l.draft_id)
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR NO KEY UPDATE OF s
), probe AS MATERIALIZED (
    SELECT l.id, l.workspace_id, l.tenant_id, l.turn_id, l.fencing_token, l.created_at,
           l.run_expires_at, l.last_metered_at,
           s.id AS thread_id, s.fencing_token AS live_fence, s.run_expires_at AS thread_expires_at
    FROM thread.runs l
    JOIN locked_thread s ON s.id = COALESCE(l.fleet_id, l.draft_id)
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR UPDATE OF l
), instant AS MATERIALIZED (
    -- Read once the locks are held, so every deadline is judged after them.
    SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM probe) locked
), guard AS MATERIALIZED (
    -- Still the live holder, before either deadline, at any age. The run time since the
    -- cursor never reaches past the deadline it was granted.
    SELECT p.*, i.checked_at,
           i.checked_at + $5::interval AS capped,
           GREATEST(p.last_metered_at, LEAST(i.checked_at, p.run_expires_at)) AS metered_until
    FROM probe p
    CROSS JOIN instant i
    WHERE p.fencing_token = p.live_fence
      AND p.run_expires_at > i.checked_at AND p.thread_expires_at > i.checked_at
), moved AS (
    UPDATE thread.runs l
    SET run_expires_at = g.capped, last_metered_at = g.metered_until, updated_at = now()
    FROM guard g
    WHERE l.id = g.id
    RETURNING l.run_expires_at
), moved_thread AS (
    UPDATE thread.threads s
    SET run_expires_at = g.capped, updated_at = now()
    FROM guard g
    WHERE s.id = g.thread_id
    RETURNING s.id
)
SELECT
    (SELECT run_expires_at FROM moved) AS new_until,
    g.metered_until AS checked_at, g.created_at AS started_at,
    (EXTRACT(EPOCH FROM g.metered_until - g.last_metered_at) * $3::bigint)::bigint AS d_ms,
    -- The runtime line's number: the milliseconds from the run's start to the slice's.
    -- The cursor only moves forward under the run's lock, so no two slices share one.
    (EXTRACT(EPOCH FROM g.last_metered_at - g.created_at) * $3::bigint)::bigint AS runtime_seq,
    g.tenant_id, g.workspace_id, g.turn_id
FROM (SELECT 1) AS one
LEFT JOIN guard g ON true

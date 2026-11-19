-- The renewal both kinds share: the lease and its slot moved to one clamped deadline under
-- the lease's fence, and the runtime since the lease's cursor measured, in one statement.
-- Both rows move or neither does. The caller holds the host row and has run
-- `lock_cancelled.sql`, which takes the slot and lease locks in this order and refuses a
-- cancelled lease; the owner's own liveness check and the slice's usage line are its own
-- statements, in the same transaction.
--
-- $1 lease · $2 runner · $3 milliseconds per second · $4 active status · $5 lease TTL ·
-- $6 the most a lease may run
WITH locked_slot AS MATERIALIZED (
    SELECT s.id, s.fencing_token, s.lease_expires_at
    FROM runner.slots s JOIN runner.leases l ON s.id = COALESCE(l.fleet_id, l.draft_id)
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR UPDATE OF s
), probe AS MATERIALIZED (
    SELECT l.id, l.workspace_id, l.tenant_id, l.event_id, l.fencing_token, l.created_at,
           l.lease_expires_at, l.last_metered_at,
           l.created_at + LEAST($6::interval, make_interval(secs => l.max_runtime_seconds))
               AS hard_cap,
           s.id AS slot_id, s.fencing_token AS live_fence, s.lease_expires_at AS slot_expires_at
    FROM runner.leases l
    JOIN locked_slot s ON s.id = COALESCE(l.fleet_id, l.draft_id)
    WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
    FOR UPDATE OF l
), instant AS MATERIALIZED (
    -- Read once the locks are held, so every deadline is judged after them.
    SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM probe) locked
), guard AS MATERIALIZED (
    -- Still the live holder, before either deadline, and below the hard cap. The run time
    -- since the cursor never reaches past the deadline it was granted.
    SELECT p.*, i.checked_at,
           LEAST(i.checked_at + $5::interval, p.hard_cap) AS capped,
           GREATEST(p.last_metered_at, LEAST(i.checked_at, p.lease_expires_at)) AS metered_until
    FROM probe p
    CROSS JOIN instant i
    WHERE p.fencing_token = p.live_fence
      AND p.hard_cap > i.checked_at
      AND p.lease_expires_at > i.checked_at AND p.slot_expires_at > i.checked_at
), moved AS (
    UPDATE runner.leases l
    SET lease_expires_at = g.capped, last_metered_at = g.metered_until, updated_at = now()
    FROM guard g
    WHERE l.id = g.id
    RETURNING l.lease_expires_at
), slot AS (
    UPDATE runner.slots s
    SET lease_expires_at = g.capped, updated_at = now()
    FROM guard g
    WHERE s.id = g.slot_id
    RETURNING s.id
)
SELECT
    (SELECT count(*) FROM probe)::bigint AS probe_found,
    (SELECT lease_expires_at FROM moved) AS new_until,
    (SELECT hard_cap FROM probe) AS hard_cap,
    g.metered_until AS checked_at, g.created_at AS started_at,
    (EXTRACT(EPOCH FROM g.metered_until - g.last_metered_at) * $3::bigint)::bigint AS d_ms,
    -- The runtime line's number: the milliseconds from the lease's start to the slice's.
    -- The cursor only moves forward under the lease's lock, so no two slices share one.
    (EXTRACT(EPOCH FROM g.last_metered_at - g.created_at) * $3::bigint)::bigint AS runtime_seq,
    g.tenant_id, g.workspace_id, g.event_id
FROM (SELECT 1) AS one
LEFT JOIN guard g ON true

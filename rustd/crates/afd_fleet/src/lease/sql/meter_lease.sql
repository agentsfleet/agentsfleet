WITH locked_slot AS MATERIALIZED (
    SELECT s.fleet_id, s.fencing_token, s.lease_expires_at
    FROM runner.fleet_slots s JOIN runner.leases l ON l.fleet_id = s.fleet_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $4
    FOR UPDATE OF s
), probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, e.event_id,
           l.fencing_token, l.created_at, l.lease_expires_at, l.last_metered_at,
           a.fencing_token AS live_fence, a.lease_expires_at AS slot_expires_at
    FROM runner.leases l
    JOIN locked_slot a ON a.fleet_id = l.fleet_id
    JOIN fleet.events e ON e.id = l.fleet_event_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $4
    FOR UPDATE OF l
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM probe
), metered AS (
    -- The run time since the lease's cursor, never past the deadline it was
    -- granted: a report that lands after its lease lapsed bills no time beyond it.
    SELECT p.*, i.checked_at,
           GREATEST(p.last_metered_at, LEAST(i.checked_at, p.lease_expires_at)) AS metered_until
    FROM probe p CROSS JOIN instant i
), guard AS (
    -- `$5` chooses the ending. A renewal must still be the live holder before
    -- either deadline and below the hard cap; a settle needs only that no
    -- reclaim has superseded it, because a report may land after its lease lapsed.
    SELECT *, LEAST(checked_at + $6::interval, created_at + $7::interval) AS capped,
           (EXTRACT(EPOCH FROM metered_until - last_metered_at) * $3::bigint)::bigint AS d_ms
    FROM metered
    WHERE CASE WHEN $5::boolean
               THEN fencing_token >= live_fence
               ELSE fencing_token = live_fence
                    AND LEAST(checked_at + $6::interval, created_at + $7::interval) > checked_at
                    AND lease_expires_at > checked_at AND slot_expires_at > checked_at
          END
      AND EXISTS (
          SELECT 1 FROM fleet.admissions a
          WHERE a.id = metered.event_id::uuid AND a.fleet_id = metered.fleet_id
            AND a.lease_fence = metered.fencing_token AND a.settled_at IS NULL
      )
), moved AS (
    UPDATE runner.leases l
    SET status           = CASE WHEN $5::boolean THEN $8 ELSE l.status END,
        lease_expires_at = CASE WHEN $5::boolean THEN l.lease_expires_at ELSE g.capped END,
        last_metered_at  = g.metered_until,
        updated_at = now()
    FROM guard g WHERE l.id = g.id
    RETURNING l.lease_expires_at AS capped
), slot AS (
    UPDATE runner.fleet_slots a
    SET lease_expires_at = CASE WHEN $5::boolean THEN a.lease_expires_at ELSE g.capped END,
        updated_at = now()
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
), settled AS (
    UPDATE fleet.admissions a
    SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
    FROM guard g
    WHERE $5::boolean AND a.id = g.event_id::uuid AND a.fleet_id = g.fleet_id
      AND a.lease_fence = g.fencing_token AND a.settled_at IS NULL
      AND EXISTS (SELECT 1 FROM moved)
), tally AS (
    INSERT INTO runner.lifetime_counters AS c
      (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
       created_at, updated_at)
    SELECT $2::uuid, 0,
           CASE WHEN $9::boolean THEN 1 ELSE 0 END,
           CASE WHEN $9::boolean THEN 0 ELSE 1 END,
           0, now(), now()
    FROM moved
    WHERE $5::boolean
    ON CONFLICT (runner_id) DO UPDATE
       SET succeeded_count = c.succeeded_count + EXCLUDED.succeeded_count,
           failed_count    = c.failed_count + EXCLUDED.failed_count,
           updated_at      = now()
)
SELECT
    (SELECT count(*) FROM probe)::bigint          AS probe_found,
    (SELECT count(*) FROM moved)::bigint          AS claimed,
    (SELECT capped FROM moved)                    AS new_until,
    (SELECT created_at + $7::interval FROM probe) AS hard_cap,
    g.metered_until AS checked_at, g.created_at AS started_at, g.d_ms,
    g.tenant_id::text AS tenant_id, g.workspace_id::text AS workspace_id,
    g.fleet_id::text AS fleet_id, g.event_id,
    (SELECT f.name FROM fleet.fleets f WHERE f.id = g.fleet_id) AS fleet_name,
    (SELECT count(*) FROM billing.usage_lines u
      WHERE u.work_type = $10 AND u.work_id = g.id AND u.seq >= $11)::bigint AS daemon_lines
FROM (SELECT 1) AS one
LEFT JOIN guard g ON true

WITH probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, e.event_id,
           l.fencing_token, l.created_at, l.lease_expires_at,
           a.fencing_token AS live_fence, a.lease_expires_at AS slot_expires_at,
           a.last_metered_at, a.metered_input_token_count, a.metered_cached_token_count,
           a.metered_output_token_count
    FROM runner.leases l
    JOIN runner.fleet_slots a ON a.fleet_id = l.fleet_id
    JOIN fleet.events e ON e.id = l.fleet_event_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $7
    FOR UPDATE OF l, a
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM probe
), metered AS (
    SELECT p.*, i.checked_at,
           (EXTRACT(EPOCH FROM GREATEST(interval '0', i.checked_at - p.last_metered_at))
              * $6::bigint)::bigint                               AS d_ms,
           GREATEST(0, $3::bigint - p.metered_input_token_count)  AS d_in,
           GREATEST(0, $4::bigint - p.metered_cached_token_count) AS d_cached,
           GREATEST(0, $5::bigint - p.metered_output_token_count) AS d_out
    FROM probe p CROSS JOIN instant i
), guard AS (
    -- `$8` chooses the ending. A renewal must still be the live holder before
    -- either deadline and below the hard cap; a settle needs only that no
    -- reclaim has superseded it, because a report may land after its lease lapsed.
    SELECT *, LEAST(checked_at + $9::interval, created_at + $10::interval) AS capped
    FROM metered
    WHERE CASE WHEN $8::boolean
               THEN fencing_token >= live_fence
               ELSE fencing_token = live_fence
                    AND LEAST(checked_at + $9::interval, created_at + $10::interval) > checked_at
                    AND lease_expires_at > checked_at AND slot_expires_at > checked_at
          END
), moved AS (
    UPDATE runner.leases l
    SET status           = CASE WHEN $8::boolean THEN $11 ELSE l.status END,
        lease_expires_at = CASE WHEN $8::boolean THEN l.lease_expires_at ELSE g.capped END,
        updated_at = now(),
        metered_input_token_count  = GREATEST(l.metered_input_token_count, $3),
        metered_cached_token_count = GREATEST(l.metered_cached_token_count, $4),
        metered_output_token_count = GREATEST(l.metered_output_token_count, $5)
    FROM guard g WHERE l.id = g.id
    RETURNING l.lease_expires_at AS capped
), slot AS (
    UPDATE runner.fleet_slots a
    SET lease_expires_at = CASE WHEN $8::boolean THEN a.lease_expires_at ELSE g.capped END,
        metered_input_token_count  = GREATEST(a.metered_input_token_count, $3),
        metered_cached_token_count = GREATEST(a.metered_cached_token_count, $4),
        metered_output_token_count = GREATEST(a.metered_output_token_count, $5),
        last_metered_at = g.checked_at, updated_at = now()
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
), tally AS (
    INSERT INTO runner.lifetime_counters AS c
      (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
       created_at, updated_at)
    SELECT $2::uuid, 0,
           CASE WHEN $12::boolean THEN 1 ELSE 0 END,
           CASE WHEN $12::boolean THEN 0 ELSE 1 END,
           0, now(), now()
    FROM moved
    WHERE $8::boolean
    ON CONFLICT (runner_id) DO UPDATE
       SET succeeded_count = c.succeeded_count + EXCLUDED.succeeded_count,
           failed_count    = c.failed_count + EXCLUDED.failed_count,
           updated_at      = now()
)
SELECT
    (SELECT count(*) FROM probe)::bigint           AS probe_found,
    (SELECT count(*) FROM moved)::bigint           AS claimed,
    (SELECT capped FROM moved)                     AS new_until,
    (SELECT created_at + $10::interval FROM probe) AS hard_cap,
    g.checked_at, g.created_at AS started_at, g.d_ms, g.d_in, g.d_cached, g.d_out,
    g.tenant_id::text AS tenant_id, g.workspace_id::text AS workspace_id,
    g.fleet_id::text AS fleet_id, g.event_id,
    (SELECT f.name FROM fleet.fleets f WHERE f.id = g.fleet_id) AS fleet_name,
    (SELECT count(*) FROM billing.usage_lines u
      WHERE u.work_type = $13 AND u.work_id = g.id)::bigint AS logged_lines
FROM (SELECT 1) AS one
LEFT JOIN guard g ON true

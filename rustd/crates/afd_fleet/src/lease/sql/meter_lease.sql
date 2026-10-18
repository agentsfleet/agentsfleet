WITH probe AS (
    SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, e.event_id, l.event_created_at,
           l.posture, l.model, l.fencing_token, l.created_at, l.lease_expires_at,
           a.fencing_token AS live_fence, a.lease_expires_at AS slot_expires_at,
           a.last_metered_at, a.metered_input_token_count, a.metered_cached_token_count,
           a.metered_output_token_count
    FROM runner.leases l
    JOIN runner.fleet_slots a ON a.fleet_id = l.fleet_id
    JOIN fleet.events e ON e.id = l.fleet_event_id
    WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.status = $14
    FOR UPDATE OF l, a
), instant AS MATERIALIZED (
    SELECT clock_timestamp() AS checked_at FROM probe
), metered AS (
    SELECT p.*, i.checked_at,
           (EXTRACT(EPOCH FROM GREATEST(interval '0', i.checked_at - p.last_metered_at))
              * $11::bigint)::bigint                               AS d_ms,
           GREATEST(0, $3::bigint - p.metered_input_token_count)  AS d_in,
           GREATEST(0, $4::bigint - p.metered_cached_token_count) AS d_cached,
           GREATEST(0, $5::bigint - p.metered_output_token_count) AS d_out
    FROM probe p CROSS JOIN instant i
), bal AS (
    SELECT w.tenant_id, w.balance_nanos AS bal0
    FROM billing.wallets w
    JOIN metered m ON m.tenant_id = w.tenant_id
    FOR UPDATE OF w
), calc AS (
    SELECT m.*, b.bal0,
           (m.d_ms * $6::bigint) / $11::bigint      AS run_fee,
           (m.d_in * $7::bigint) / $12::bigint
             + (m.d_cached * $8::bigint) / $12::bigint
             + (m.d_out * $9::bigint) / $12::bigint AS token_cost
    FROM metered m
    LEFT JOIN bal b ON b.tenant_id = m.tenant_id
), guard AS (
    -- `$15` chooses the ending. A renewal must still be the live holder before
    -- either deadline and below the hard cap; a settle needs only that no
    -- reclaim has superseded it, because a report may land after its lease lapsed.
    SELECT *, run_fee + token_cost AS slice,
           LEAST(run_fee + token_cost, COALESCE(bal0, run_fee + token_cost)) AS charged,
           LEAST(checked_at + $16::interval, created_at + $17::interval)   AS capped
    FROM calc
    WHERE CASE WHEN $15::boolean
               THEN fencing_token >= live_fence
               ELSE fencing_token = live_fence
                    AND LEAST(checked_at + $16::interval, created_at + $17::interval) > checked_at
                    AND lease_expires_at > checked_at AND slot_expires_at > checked_at
          END
), moved AS (
    UPDATE runner.leases l
    SET status           = CASE WHEN $15::boolean THEN $18 ELSE l.status END,
        lease_expires_at = CASE WHEN $15::boolean THEN l.lease_expires_at ELSE g.capped END,
        updated_at = now(),
        metered_input_token_count  = GREATEST(l.metered_input_token_count, $3),
        metered_cached_token_count = GREATEST(l.metered_cached_token_count, $4),
        metered_output_token_count = GREATEST(l.metered_output_token_count, $5)
    FROM guard g WHERE l.id = g.id
    RETURNING l.lease_expires_at AS capped, g.charged
), slot AS (
    UPDATE runner.fleet_slots a
    SET lease_expires_at = CASE WHEN $15::boolean THEN a.lease_expires_at ELSE g.capped END,
        metered_input_token_count  = GREATEST(a.metered_input_token_count, $3),
        metered_cached_token_count = GREATEST(a.metered_cached_token_count, $4),
        metered_output_token_count = GREATEST(a.metered_output_token_count, $5),
        last_metered_at = g.checked_at, updated_at = now()
    FROM guard g WHERE a.fleet_id = g.fleet_id
    RETURNING a.fleet_id
), wallet AS (
    UPDATE billing.wallets w
    SET balance_nanos = GREATEST(0, w.balance_nanos - g.slice),
        exhausted_at = CASE WHEN w.balance_nanos - g.slice <= 0
                            THEN COALESCE(w.exhausted_at, now()) ELSE NULL END,
        updated_at = now()
    FROM guard g WHERE w.tenant_id = g.tenant_id
    RETURNING w.tenant_id
), ledger AS (
    INSERT INTO billing.fleet_charges AS c
      (id, tenant_id, workspace_id, fleet_id, fleet_name, event_id, charge_type, posture,
       model, charged_nanos, input_token_count, cached_input_token_count, output_token_count,
       wall_ms, event_created_at, created_at, updated_at)
    SELECT $13::uuid, g.tenant_id, g.workspace_id, g.fleet_id,
           (SELECT f.name FROM fleet.fleets f WHERE f.id = g.fleet_id),
           g.event_id, $10, g.posture, g.model, g.charged, g.d_in, g.d_cached, g.d_out,
           g.d_ms, g.event_created_at, g.checked_at, g.checked_at
    FROM guard g
    ON CONFLICT (event_id, charge_type, fleet_id) DO UPDATE SET
        charged_nanos = c.charged_nanos + EXCLUDED.charged_nanos,
        input_token_count = COALESCE(c.input_token_count, 0) + EXCLUDED.input_token_count,
        cached_input_token_count = COALESCE(c.cached_input_token_count, 0)
            + EXCLUDED.cached_input_token_count,
        output_token_count = COALESCE(c.output_token_count, 0) + EXCLUDED.output_token_count,
        wall_ms = COALESCE(c.wall_ms, 0) + EXCLUDED.wall_ms,
        updated_at = GREATEST(c.updated_at, EXCLUDED.updated_at)
    -- `fleet_name` is absent from this SET list on purpose: the name is the
    -- snapshot taken when the charge was first written, and every renewal of a
    -- run lands here, so listing it would rewrite the charge's history on a
    -- mid-run rename.
    RETURNING c.event_id
), tally AS (
    INSERT INTO runner.lifetime_counters AS c
      (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
       created_at, updated_at)
    SELECT $2::uuid, 0,
           CASE WHEN $19::boolean THEN 1 ELSE 0 END,
           CASE WHEN $19::boolean THEN 0 ELSE 1 END,
           0, now(), now()
    FROM moved
    WHERE $15::boolean
    ON CONFLICT (runner_id) DO UPDATE
       SET succeeded_count = c.succeeded_count + EXCLUDED.succeeded_count,
           failed_count    = c.failed_count + EXCLUDED.failed_count,
           updated_at      = now()
)
SELECT
    (SELECT count(*) FROM probe)::bigint           AS probe_found,
    (SELECT count(*) FROM moved)::bigint           AS claimed,
    (SELECT capped FROM moved)                     AS new_until,
    (SELECT created_at + $17::interval FROM probe) AS hard_cap,
    (SELECT charged FROM moved)                    AS charged_nanos

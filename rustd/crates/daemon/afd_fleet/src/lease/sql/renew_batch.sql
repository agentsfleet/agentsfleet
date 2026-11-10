-- One host's renewals in one statement (M245 §7): every live lease the runner
-- names, extended and metered together, each under its own fence. A lease that
-- fails a check extends nothing and changes no other lease's answer.
--
-- Lock order is a renewal's and a report's: the host row shared, then the slot,
-- then the lease, taken in fleet order so two statements never wait on each
-- other in a cycle. The host row is read FOR SHARE, never exclusively: renewals
-- never wait on each other, but a credential rotation or supersession updating
-- the row fences every renewal behind it, which re-reads the new credential and
-- refuses the old connection, as `lock_host.sql` does under its exclusive lock.
-- A renewal adds no occupancy, so a census read beside a claim is harmless. A
-- lease runs a durable task only on the per-lease path, so this refuses one.
--
-- Guarded per lease on the posture, provider, model and catalogue version its
-- request lines were priced at, and on the head revision its ceilings were
-- read from: a lease whose facts moved writes nothing, and its row says so
-- for the caller's one rerun.
--
-- $1 runner · $2 leases · per lease, in $2's order: $3 posture · $4 provider ·
-- $5 model · $6 catalogue version priced at (NULL: none priced from it) ·
-- $7 runtime price · $8 runtime line id · $9 daily ceiling (NULL: none read) ·
-- $10 monthly ceiling · $11 head revision the ceilings came from ·
-- request lines, in parallel: $12 lease · $13 id · $14 seq · $15 requested at ·
-- $16-$21 input, cached, write, 5-minute write, 1-hour write, output ·
-- $22 price · $23 unpriced tokens · $24 charged ·
-- $25 active status · $26 milliseconds per second · $27 run nanos per second ·
-- $28 lease TTL · $29 the most a lease may run · $30 the lease work type ·
-- $31 model line kind · $32 runtime line kind · $33 the most milliseconds one
-- line carries · $34 the rolling day · $35 the host states that renew ·
-- $36 the opening's credential digest (NULL: a trusted domain call) ·
-- $37 the opening's connection
WITH asked AS (
  SELECT *
  FROM unnest($2::uuid[], $3::text[], $4::text[], $5::text[], $6::bigint[], $7::uuid[],
              $8::uuid[], $9::bigint[], $10::bigint[], $11::uuid[])
    AS a(lease_id, posture, provider, model, catalogue_version, runtime_price,
         runtime_line_id, daily_ceiling, monthly_ceiling, head_revision)
), host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r
  WHERE r.id = $1 AND r.degraded_reason IS NULL AND r.status = ANY($35::text[])
    AND ((r.token_hash = $36::text AND r.connection_id IS NOT DISTINCT FROM $37::uuid)
         OR $36::text IS NULL)
    AND (SELECT count(*) FROM runner.leases l
          WHERE l.runner_id = $1 AND l.status = $25 AND l.lease_expires_at > now())
      + (SELECT count(*) FROM authoring.attempts a
          WHERE a.runner_id = $1 AND a.status = $25 AND a.lease_expires_at > now())
      <= r.worker_count
  FOR SHARE OF r
), locked_slot AS MATERIALIZED (
  SELECT s.fleet_id, s.fencing_token, s.lease_expires_at
  FROM runner.fleet_slots s
  JOIN runner.leases l ON l.fleet_id = s.fleet_id
  JOIN asked a ON a.lease_id = l.id
  WHERE l.runner_id = $1 AND l.status = $25
  ORDER BY s.fleet_id
  FOR UPDATE OF s
), probe AS MATERIALIZED (
  SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id, l.fencing_token,
         l.created_at, l.lease_expires_at, l.last_metered_at, l.posture, l.provider,
         l.model, l.task_id, l.cancel_requested_at IS NOT NULL AS cancelled,
         s.fencing_token AS live_fence, s.lease_expires_at AS slot_expires_at
  FROM runner.leases l
  JOIN locked_slot s ON s.fleet_id = l.fleet_id
  JOIN asked a ON a.lease_id = l.id
  WHERE l.runner_id = $1 AND l.status = $25
  ORDER BY l.fleet_id
  FOR UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the locks are held, so every deadline is judged after them.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM probe) locked
), catalogue AS MATERIALIZED (
  SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1
), floors AS MATERIALIZED (
  SELECT date_trunc('milliseconds', now() - $34::interval) AS day_floor,
         date_trunc('month', now(), 'UTC') AS month_floor
), spend AS MATERIALIZED (
  -- Each fleet with a ceiling, summed as `select_fleet_spend.sql` sums it.
  SELECT c.fleet_id,
         COALESCE(SUM(u.charged_nanos) FILTER (WHERE u.created_at >= fl.day_floor), 0)::bigint
             AS day_nanos,
         COALESCE(SUM(u.charged_nanos) FILTER (WHERE u.created_at >= fl.month_floor), 0)::bigint
             AS month_nanos
  FROM (SELECT DISTINCT p.fleet_id, p.workspace_id
        FROM probe p JOIN asked a ON a.lease_id = p.id
        WHERE a.daily_ceiling IS NOT NULL) c
  CROSS JOIN floors fl
  LEFT JOIN billing.usage_lines u
    ON u.workspace_id = c.workspace_id AND u.fleet_id = c.fleet_id
   AND u.created_at >= LEAST(fl.day_floor, fl.month_floor)
  GROUP BY c.fleet_id
), judged AS MATERIALIZED (
  SELECT p.*, a.runtime_price, a.runtime_line_id, i.checked_at,
         LEAST(i.checked_at + $28::interval, p.created_at + $29::interval) AS capped,
         GREATEST(p.last_metered_at, LEAST(i.checked_at, p.lease_expires_at)) AS metered_until,
         p.fencing_token = p.live_fence AND p.lease_expires_at > i.checked_at
             AND p.slot_expires_at > i.checked_at AS fence_holds,
         p.created_at + $29::interval > i.checked_at AS below_cap,
         p.posture = a.posture AND p.provider = a.provider AND p.model = a.model
             AND (a.catalogue_version IS NULL
                  OR a.catalogue_version = (SELECT resource_version FROM catalogue))
             AS priced_as_held,
         a.daily_ceiling IS NULL OR f.head_revision_id IS NOT DISTINCT FROM a.head_revision
             AS ceilings_current,
         -- Reached, not passed: spend at a ceiling stops the run (`budget::covers`).
         a.daily_ceiling IS NOT NULL
             AND (COALESCE(sp.day_nanos, 0) >= a.daily_ceiling
                  OR (a.monthly_ceiling IS NOT NULL
                      AND COALESCE(sp.month_nanos, 0) >= a.monthly_ceiling))
             AS over_budget,
         EXISTS (SELECT 1 FROM host) AS host_renews,
         EXISTS (SELECT 1 FROM fleet.admissions ad
                 WHERE ad.id = p.event_id AND ad.fleet_id = p.fleet_id
                   AND ad.lease_fence = p.fencing_token AND ad.settled_at IS NULL)
             AS admitted
  FROM probe p
  JOIN asked a ON a.lease_id = p.id
  CROSS JOIN instant i
  JOIN fleet.fleets f ON f.id = p.fleet_id
  LEFT JOIN spend sp ON sp.fleet_id = p.fleet_id
), guard AS MATERIALIZED (
  -- Capped as the rater caps a line's milliseconds ($33).
  SELECT j.*,
         LEAST((EXTRACT(EPOCH FROM j.metered_until - j.last_metered_at) * $26::bigint)::bigint,
               $33::bigint) AS d_ms
  FROM judged j
  WHERE j.fence_holds AND j.below_cap AND NOT j.cancelled AND j.task_id IS NULL
    AND j.priced_as_held AND j.ceilings_current AND NOT j.over_budget
    AND j.host_renews AND j.admitted
), runtime AS MATERIALIZED (
  -- Numbered by the slice's start, in milliseconds from the lease's start, and
  -- charged as the rater charges run time: whole nanos, rounded up.
  SELECT g.id AS lease_id, g.runtime_line_id AS id, g.runtime_price,
         (EXTRACT(EPOCH FROM g.last_metered_at - g.created_at) * $26::bigint)::bigint AS seq,
         g.metered_until AS requested_at, g.d_ms AS runtime_ms,
         (g.d_ms * $27::bigint + $26::bigint - 1) / $26::bigint AS charged
  FROM guard g
  WHERE g.d_ms > 0
), moved AS (
  UPDATE runner.leases l
  SET lease_expires_at = g.capped, last_metered_at = g.metered_until, updated_at = now()
  FROM guard g
  WHERE l.id = g.id
  RETURNING l.id, l.lease_expires_at
), slot AS (
  UPDATE runner.fleet_slots s
  SET lease_expires_at = g.capped, updated_at = now()
  FROM guard g
  WHERE s.fleet_id = g.fleet_id AND s.fencing_token = g.fencing_token
  RETURNING s.fleet_id
), lines AS (
  INSERT INTO billing.usage_lines
      (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name,
       event_id, authoring_attempt_id, provider, model, posture, requested_at,
       input_token_count, cached_input_token_count, cache_write_token_count,
       cache_write_5m_token_count, cache_write_1h_token_count, output_token_count, runtime_ms,
       platform_model_price_id, unpriced_token_count, charged_nanos, created_at, updated_at)
  SELECT line.id, $30, g.id, line.kind, line.seq, g.tenant_id, g.workspace_id, g.fleet_id,
         f.name, g.event_id, NULL, g.provider, g.model, g.posture, line.requested_at,
         line.input, line.cached, line.write, line.write_5m, line.write_1h, line.output,
         line.runtime_ms, line.price_id, line.unpriced, line.charged, now(), now()
  FROM (
      SELECT requested.lease_id, requested.id, $31::text AS kind, requested.seq,
             requested.requested_at, requested.input, requested.cached, requested.write,
             requested.write_5m, requested.write_1h, requested.output, 0::bigint AS runtime_ms,
             requested.price_id, requested.unpriced, requested.charged
      FROM unnest($12::uuid[], $13::uuid[], $14::bigint[], $15::timestamptz[], $16::bigint[],
                  $17::bigint[], $18::bigint[], $19::bigint[], $20::bigint[], $21::bigint[],
                  $22::uuid[], $23::bigint[], $24::bigint[])
          AS requested(lease_id, id, seq, requested_at, input, cached, write, write_5m,
                       write_1h, output, price_id, unpriced, charged)
      UNION ALL
      SELECT r.lease_id, r.id, $32::text, r.seq, r.requested_at, 0, 0, 0, 0, 0, 0,
             r.runtime_ms, r.runtime_price, 0, r.charged
      FROM runtime r
  ) line
  JOIN guard g ON g.id = line.lease_id
  JOIN fleet.fleets f ON f.id = g.fleet_id
  ON CONFLICT (work_type, work_id, kind, seq) DO NOTHING
  RETURNING work_id, charged_nanos
)
-- One row per lease asked about, in any order; a lease the probe never found
-- answers with every check NULL.
SELECT a.lease_id,
       j.id IS NOT NULL AS found,
       j.cancelled, j.fence_holds, j.below_cap, j.priced_as_held, j.ceilings_current,
       j.over_budget, j.host_renews, j.admitted,
       j.task_id IS NOT NULL AS has_task,
       m.lease_expires_at AS extended_until,
       (SELECT s.fleet_id FROM slot s WHERE s.fleet_id = j.fleet_id) IS NOT NULL AS slot_moved,
       COALESCE((SELECT sum(ln.charged_nanos) FROM lines ln WHERE ln.work_id = a.lease_id), 0)::bigint
           AS charged_nanos
FROM asked a
LEFT JOIN judged j ON j.id = a.lease_id
LEFT JOIN moved m ON m.id = a.lease_id

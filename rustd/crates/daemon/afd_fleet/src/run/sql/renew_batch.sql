-- The fleet's host-wide renewal: `meter_run.sql` for every run a frame names, with its
-- turns and usage lines. A change to the shared
-- bookkeeping belongs in both.
--
-- One host's renewals in one statement: every live run the runner
-- names, extended and metered together, each under its own fence. A run that
-- fails a check extends nothing and changes no other run's answer.
--
-- Lock order is a renewal's and a report's: the host row shared, then the thread,
-- then the run, taken in fleet order so two statements never wait on each
-- other in a cycle. The host row is read FOR SHARE, never exclusively: renewals
-- never wait on each other, but a credential rotation or supersession updating
-- the row fences every renewal behind it, which re-reads the new credential and
-- refuses the old connection, as `lock_host.sql` does under its exclusive lock.
-- A renewal adds no occupancy, so a census read beside an assignment is harmless. A
-- run runs a durable task only on the per-run path, so this refuses one.
--
-- Guarded per run on the posture, provider, model and catalogue version its
-- request lines were priced at: a run whose facts moved writes nothing, and its
-- row says so for the caller's one rerun.
--
-- $1 runner · $2 runs · per run, in $2's order: $3 posture · $4 provider ·
-- $5 model · $6 catalogue version priced at (NULL: none priced from it) ·
-- $7 runtime price · $8 runtime line id ·
-- request lines, in parallel: $9 run · $10 id · $11 seq · $12 requested at ·
-- $13-$18 input, cached, write, 5-minute write, 1-hour write, output ·
-- $19 price · $20 unpriced tokens · $21 charged ·
-- $22 active status · $23 milliseconds per second · $24 run nanos per second ·
-- $25 run TTL · $26 the run work type ·
-- $27 model line kind · $28 runtime line kind · $29 the most milliseconds one
-- line carries · $30 the host states that renew ·
-- $31 the opening's credential digest (NULL: a trusted domain call) ·
-- $32 the opening's connection · $33 whether the call is a stream's
WITH asked AS (
  SELECT *
  FROM unnest($2::uuid[], $3::text[], $4::text[], $5::text[], $6::bigint[], $7::uuid[],
              $8::uuid[])
    AS a(run_id, posture, provider, model, catalogue_version, runtime_price, runtime_line_id)
), host AS MATERIALIZED (
  SELECT r.id, r.worker_count
  FROM runner.runners r
  WHERE r.id = $1 AND r.degraded_reason IS NULL AND r.status = ANY($30::text[])
    -- A stream call ($33) must prove the opening credential and connection; a
    -- trusted call was proven upstream. A stream call without its proof is refused.
    AND (NOT $33::boolean
         OR (r.token_hash = $31::text AND r.connection_id IS NOT DISTINCT FROM $32::uuid))
  FOR SHARE OF r
), census AS MATERIALIZED (
  -- Live runs are counted against a clock read once the host row is held, so
  -- a wait on a credential rotation cannot count a lapsed run as live.
  SELECT h.id
  FROM host h
  CROSS JOIN (SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM host) held) i
  WHERE (SELECT count(*) FROM thread.runs l
          WHERE l.runner_id = $1 AND l.status = $22 AND l.run_expires_at > i.checked_at)
      <= h.worker_count
), locked_thread AS MATERIALIZED (
  -- Joined to `census`, so the host row is held FOR SHARE before any thread: a
  -- CTE runs when first read, and an issue holds the host row and then a thread.
  -- A host that may not renew locks nothing, and every run answers lost.
  SELECT s.fleet_id, s.fencing_token, s.run_expires_at
  FROM thread.threads s
  JOIN thread.runs l ON l.fleet_id = s.fleet_id
  JOIN asked a ON a.run_id = l.id
  CROSS JOIN census h
  WHERE l.runner_id = $1 AND l.status = $22
  ORDER BY s.fleet_id
  FOR NO KEY UPDATE OF s
), probe AS MATERIALIZED (
  SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.turn_id, l.fencing_token,
         l.created_at, l.run_expires_at, l.last_metered_at, l.posture, l.provider,
         l.model, l.durable_task_id, l.cancel_requested_at IS NOT NULL AS cancelled,
         s.fencing_token AS live_fence, s.run_expires_at AS thread_expires_at
  FROM thread.runs l
  JOIN locked_thread s ON s.fleet_id = l.fleet_id
  JOIN asked a ON a.run_id = l.id
  WHERE l.runner_id = $1 AND l.status = $22
  ORDER BY l.fleet_id
  FOR NO KEY UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the locks are held, so every deadline is judged after them.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM probe) locked
), catalogue AS MATERIALIZED (
  SELECT c.number FROM model.revision c WHERE c.id = 1
), judged AS MATERIALIZED (
  SELECT p.*, a.runtime_price, a.runtime_line_id, i.checked_at,
         i.checked_at + $25::interval AS capped,
         GREATEST(p.last_metered_at, LEAST(i.checked_at, p.run_expires_at)) AS metered_until,
         p.fencing_token = p.live_fence AND p.run_expires_at > i.checked_at
             AND p.thread_expires_at > i.checked_at AS fence_holds,
         p.posture = a.posture AND p.provider = a.provider AND p.model = a.model
             AND (a.catalogue_version IS NULL
                  OR a.catalogue_version = (SELECT number FROM catalogue))
             AS priced_as_held,
         EXISTS (SELECT 1 FROM census) AS host_renews,
         EXISTS (SELECT 1 FROM thread.turns ad
                 WHERE ad.id = p.turn_id AND ad.thread_id = p.fleet_id
                   AND ad.run_fence = p.fencing_token AND ad.settled_at IS NULL)
             AS admitted
  FROM probe p
  JOIN asked a ON a.run_id = p.id
  CROSS JOIN instant i
), guard AS MATERIALIZED (
  -- Capped at the longest runtime one line may claim ($29), which is where the
  -- rater caps it too, so the charge here is the one the per-run path makes.
  SELECT j.*,
         LEAST((EXTRACT(EPOCH FROM j.metered_until - j.last_metered_at) * $23::bigint)::bigint,
               $29::bigint) AS d_ms
  FROM judged j
  WHERE j.fence_holds AND NOT j.cancelled AND j.durable_task_id IS NULL
    AND j.priced_as_held AND j.host_renews AND j.admitted
), runtime AS MATERIALIZED (
  -- Numbered by the slice's start, in milliseconds from the run's start, and
  -- charged as the rater charges run time: whole nanos, rounded up.
  SELECT g.id AS run_id, g.runtime_line_id AS id, g.runtime_price,
         (EXTRACT(EPOCH FROM g.last_metered_at - g.created_at) * $23::bigint)::bigint AS seq,
         g.metered_until AS requested_at, g.d_ms AS runtime_ms,
         (g.d_ms * $24::bigint + $23::bigint - 1) / $23::bigint AS charged
  FROM guard g
  WHERE g.d_ms > 0
), moved AS (
  UPDATE thread.runs l
  SET run_expires_at = g.capped, last_metered_at = g.metered_until, updated_at = now()
  FROM guard g
  WHERE l.id = g.id
  RETURNING l.id, l.run_expires_at
), moved_thread AS (
  UPDATE thread.threads s
  SET run_expires_at = g.capped, updated_at = now()
  FROM guard g
  WHERE s.fleet_id = g.fleet_id AND s.fencing_token = g.fencing_token
  RETURNING s.fleet_id
), lines AS (
  INSERT INTO billing.usage_lines
      (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name,
       turn_id, run_id, provider, model, posture, requested_at,
       input_token_count, cached_input_token_count, cache_write_token_count,
       cache_write_5m_token_count, cache_write_1h_token_count, output_token_count, runtime_ms,
       platform_model_price_id, unpriced_token_count, charged_nanos, created_at)
  SELECT line.id, $26, g.id, line.kind, line.seq, g.tenant_id, g.workspace_id, g.fleet_id,
         f.name, g.turn_id, NULL, g.provider, g.model, g.posture, line.requested_at,
         line.input, line.cached, line.write, line.write_5m, line.write_1h, line.output,
         line.runtime_ms, line.price_id, line.unpriced, line.charged, now()
  FROM (
      SELECT requested.run_id, requested.id, $27::text AS kind, requested.seq,
             requested.requested_at, requested.input, requested.cached, requested.write,
             requested.write_5m, requested.write_1h, requested.output, 0::bigint AS runtime_ms,
             requested.price_id, requested.unpriced, requested.charged
      FROM unnest($9::uuid[], $10::uuid[], $11::bigint[], $12::timestamptz[], $13::bigint[],
                  $14::bigint[], $15::bigint[], $16::bigint[], $17::bigint[], $18::bigint[],
                  $19::uuid[], $20::bigint[], $21::bigint[])
          AS requested(run_id, id, seq, requested_at, input, cached, write, write_5m,
                       write_1h, output, price_id, unpriced, charged)
      UNION ALL
      SELECT r.run_id, r.id, $28::text, r.seq, r.requested_at, 0, 0, 0, 0, 0, 0,
             r.runtime_ms, r.runtime_price, 0, r.charged
      FROM runtime r
  ) line
  JOIN guard g ON g.id = line.run_id
  JOIN fleet.fleets f ON f.id = g.fleet_id
  ON CONFLICT (work_type, work_id, kind, seq) DO NOTHING
)
-- One row per run asked about, in any order; a run the probe never found
-- answers with every check NULL.
SELECT a.run_id,
       j.id IS NOT NULL AS found,
       j.cancelled, j.fence_holds, j.priced_as_held, j.host_renews, j.admitted,
       j.durable_task_id IS NOT NULL AS has_durable_task,
       m.run_expires_at AS extended_until,
       (SELECT s.fleet_id FROM moved_thread s WHERE s.fleet_id = j.fleet_id) IS NOT NULL AS thread_moved
FROM asked a
LEFT JOIN judged j ON j.id = a.run_id
LEFT JOIN moved m ON m.id = a.run_id

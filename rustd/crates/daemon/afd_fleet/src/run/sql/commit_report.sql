-- The fleet's report: `shared/settle_run.sql` with the fleet's usage lines, turn,
-- delivery, session and tool records in the same statement. A change to the
-- shared bookkeeping belongs in both.
--
-- A terminal report in one statement: every write the report's
-- transaction made under the run's own lock. A potential sandbox hold
-- shares the host lock through commit to serialize with worker placement.
--
-- Lock the fleet's scope before the optional host, slot and run.
-- READ COMMITTED re-reads a locked row's latest version,
-- so the run's cursor is current, and the runtime line is numbered by its
-- slice's start: a racing renewal waiting on the same lock bills a later slice
-- under a later number. A common table expression cannot see its own statement's
-- writes, nor the triggers' (`trg_usage_lines_bump_budget`), so the closing
-- row's cost and counters and the run's token sums add the lines inserted here.
--
-- Guarded on the posture, provider, model and catalogue version the lines were
-- priced at: a wrong guess writes nothing, and the final row names the run's
-- own facts for the caller's one retry. When the guard fails, the row says
-- whether this runner already settled the run or lost its fence.
--
-- $1 run · $2 runner · $3 milliseconds per second · $4 active status ·
-- $5 the status the run settles in, `reported` or `stopped` · $6 succeeded · $7 the run work type ·
-- $8 the model line kind · $9 run nanos per second ·
-- $10 the runtime line's price · $11 posture · $12 provider · $13 model ·
-- $14 the catalogue version the lines were priced at ·
-- $15-$26 the runner's request lines, in parallel arrays: id, seq, requested
-- at, input, cached input, cache write, 5-minute write, 1-hour write, output,
-- price, unpriced tokens, charged · $27 the runtime line's id ·
-- $28 the event's status · $29 response, which is also the answer owed ·
-- $30 tokens · $31 wall time ·
-- $32 failure label · $33 failure detail · $34 tool calls · $35 the status an
-- open event holds · $36 the pending gate status · $37 the session document ·
-- $38 the hold the runner asked for · $39 the hold's idle cap ·
-- $40 the delivery's id · $41 its receipt ·
-- $42 the connectors an answer can be owed to · $43 the audit row's id ·
-- $44 its event type · $45 its metadata · $46 the fleet · $47 its workspace ·
-- $48 the longest runtime one line may claim · $49 the runtime line kind ·
-- $50 the largest count a bigint holds ·
-- $51 the notice owed in the answer's place when a failed run wrote none ·
-- $52-$53 the opening's credential proof and connection · $54 renewable states ·
-- $55 the status Stop writes
WITH scope AS MATERIALIZED (
  SELECT f.id
  FROM fleet.fleets f
  JOIN identity.lock_workspace($47) w ON w.id = f.workspace_id
  WHERE f.id = $46
  FOR KEY SHARE OF f
), hold_host AS MATERIALIZED (
  SELECT r.id
  FROM scope
  JOIN runner.runners r ON r.id = $2
  WHERE $38::timestamptz IS NOT NULL
    AND r.degraded_reason IS NULL AND r.status = ANY($54::text[])
    AND ($52::text IS NULL OR (r.token_hash = $52 AND r.connection_id IS NOT DISTINCT FROM $53::uuid))
  FOR SHARE OF r
), locked_slot AS MATERIALIZED (
  SELECT s.fleet_id, s.fencing_token, s.run_expires_at AS slot_expires_at,
         h.id IS NOT NULL AS hold_authorized
  FROM thread.threads s
  JOIN scope sc ON sc.id = s.fleet_id
  JOIN thread.runs l ON l.fleet_id = s.fleet_id
  LEFT JOIN hold_host h ON h.id = l.runner_id
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR NO KEY UPDATE OF s
), probe AS MATERIALIZED (
  SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.turn_id, l.fencing_token,
         l.created_at, l.run_expires_at, l.last_metered_at,
         l.posture, l.provider, l.model, l.task_id, a.fencing_token AS live_fence,
         a.slot_expires_at, a.hold_authorized
  FROM thread.runs l
  JOIN locked_slot a ON a.fleet_id = l.fleet_id
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the slot and run locks are held, so the slice is measured to
  -- the moment the report owns the run, never before a lock wait.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM probe) locked
), catalogue AS MATERIALIZED (
  SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1
), guard AS MATERIALIZED (
  SELECT p.*, GREATEST(p.last_metered_at, LEAST(i.checked_at, p.run_expires_at)) AS metered_until
  FROM probe p
  CROSS JOIN instant i
  WHERE p.fencing_token >= p.live_fence
    AND p.posture = $11 AND p.provider = $12 AND p.model = $13
    AND ($14::bigint IS NULL OR (SELECT resource_version FROM catalogue) = $14::bigint)
    -- A turn Stop settled as cancelled still lets its run settle, so the run is
    -- billed; the `settled` step below then finds no open turn and answers none.
    AND EXISTS (
        SELECT 1 FROM thread.turns a
        WHERE a.id = p.turn_id AND a.thread_id = p.fleet_id
          AND a.run_fence = p.fencing_token AND (a.settled_at IS NULL OR a.status = $55))
), measured AS MATERIALIZED (
  -- Capped at the longest runtime one line may claim ($48), which is where the
  -- rater caps it too, so the charge here is the one the per-run path makes.
  SELECT g.*,
         LEAST((EXTRACT(EPOCH FROM g.metered_until - g.last_metered_at) * $3::bigint)::bigint,
               $48::bigint) AS d_ms
  FROM guard g
), runtime AS MATERIALIZED (
  -- Charged as the rater charges run time: whole nanos, rounded up, in integers.
  -- Numbered by the slice's start, in milliseconds from the run's start.
  SELECT (EXTRACT(EPOCH FROM m.last_metered_at - m.created_at) * $3::bigint)::bigint AS seq,
         m.metered_until AS requested_at,
         m.d_ms AS runtime_ms,
         (m.d_ms * $9::bigint + $3::bigint - 1) / $3::bigint AS charged
  FROM measured m
  WHERE m.d_ms > 0
), lines AS (
  INSERT INTO billing.usage_lines
      (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name, turn_id,
       run_id, provider, model, posture, requested_at, input_token_count,
       cached_input_token_count, cache_write_token_count, cache_write_5m_token_count,
       cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id,
       unpriced_token_count, charged_nanos, created_at, updated_at)
  SELECT line.id, $7, m.id, line.kind, line.seq, m.tenant_id, m.workspace_id, m.fleet_id, f.name, m.turn_id,
         NULL, m.provider, m.model, m.posture, line.requested_at, line.input, line.cached,
         line.write, line.write_5m, line.write_1h, line.output, line.runtime_ms, line.price_id,
         line.unpriced, line.charged, now(), now()
  FROM measured m
  JOIN fleet.fleets f ON f.id = m.fleet_id
  CROSS JOIN (
      SELECT requested.id, $8::text AS kind, requested.seq, requested.requested_at, requested.input,
             requested.cached, requested.write, requested.write_5m, requested.write_1h,
             requested.output, 0::bigint AS runtime_ms, requested.price_id,
             requested.unpriced, requested.charged
      FROM unnest($15::uuid[], $16::bigint[], $17::timestamptz[], $18::bigint[],
                  $19::bigint[], $20::bigint[], $21::bigint[], $22::bigint[], $23::bigint[],
                  $24::uuid[], $25::bigint[], $26::bigint[])
          AS requested(id, seq, requested_at, input, cached, write, write_5m, write_1h,
                       output, price_id, unpriced, charged)
      UNION ALL
      SELECT $27::uuid, $49::text, r.seq, r.requested_at, 0, 0, 0, 0, 0, 0, r.runtime_ms, $10::uuid, 0,
             r.charged
      FROM runtime r
  ) line
  ON CONFLICT (work_type, work_id, kind, seq) DO NOTHING
  RETURNING kind, charged_nanos, input_token_count, cached_input_token_count,
            cache_write_token_count + cache_write_5m_token_count + cache_write_1h_token_count
                AS cache_write_tokens,
            output_token_count
), added AS MATERIALIZED (
  SELECT count(*)::bigint AS lines,
         COALESCE(sum(charged_nanos), 0)::bigint AS nanos,
         COALESCE(sum(charged_nanos) FILTER (WHERE kind = $8::text), 0)::bigint AS model_nanos,
         COALESCE(sum(input_token_count), 0)::bigint AS input_tokens,
         COALESCE(sum(cached_input_token_count), 0)::bigint AS cached_input_tokens,
         COALESCE(sum(cache_write_tokens), 0)::bigint AS cache_write_tokens,
         COALESCE(sum(output_token_count), 0)::bigint AS output_tokens
  FROM lines
), moved AS (
  UPDATE thread.runs l
  SET status = $5,
      last_metered_at = m.metered_until,
      -- The run ended at its report. Its cursor then meets its deadline, so it
      -- leaves the metering sweep's partial index instead of riding it for 30 days.
      run_expires_at = m.metered_until,
      updated_at = now()
  FROM measured m
  WHERE l.id = m.id
  RETURNING l.id, l.fleet_id, l.turn_id, l.fencing_token, l.workspace_id,
            m.run_expires_at, m.slot_expires_at, m.hold_authorized,
            (SELECT checked_at FROM instant) AS checked_at
), settled AS (
  -- Settles the turn and, while its event is still open, closes it with the run's answer:
  -- one update, since one statement may change a row once. An event already closed keeps
  -- its result.
  UPDATE thread.turns a
  SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now(),
      status = CASE WHEN a.status = $35 THEN $28 ELSE a.status END,
      answer_text = CASE WHEN a.status = $35 THEN $29 ELSE a.answer_text END,
      token_count = CASE WHEN a.status = $35 THEN $30 ELSE a.token_count END,
      wall_ms = CASE WHEN a.status = $35 THEN $31 ELSE a.wall_ms END,
      failure_reason = CASE WHEN a.status = $35 THEN $32 ELSE a.failure_reason END,
      failure_detail = CASE WHEN a.status = $35 THEN $33 ELSE a.failure_detail END,
      tool_calls = CASE WHEN a.status = $35 THEN $34::jsonb ELSE a.tool_calls END
  FROM moved m
  WHERE a.id = m.turn_id AND a.thread_id = m.fleet_id
    AND a.run_fence = m.fencing_token AND a.settled_at IS NULL
  RETURNING WITH (OLD AS prior)
            a.reply_provider, a.reply_address, prior.status = $35 AS closed,
            a.thread_id AS fleet_id, a.id AS turn_id, a.workspace_id, a.actor,
            a.kind AS event_type, a.status, a.token_count, a.wall_ms, a.failure_reason,
            a.failure_detail, a.resumes_turn_id AS resumes_turn_id, a.created_at, a.updated_at
), tally AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT $2, 0, CASE WHEN $6::boolean THEN 1 ELSE 0 END, CASE WHEN $6::boolean THEN 0 ELSE 1 END,
         0, now(), now()
  FROM moved
  ON CONFLICT (runner_id) DO UPDATE
     SET succeeded_count = c.succeeded_count + EXCLUDED.succeeded_count,
         failed_count    = c.failed_count + EXCLUDED.failed_count,
         updated_at      = now()
), closed AS (
  SELECT * FROM settled WHERE closed
), dropped AS (
  -- A reclaimed run's records are of a run whose answer does not stand.
  DELETE FROM thread.items t
  USING moved m
  WHERE t.fleet_id = m.fleet_id AND t.turn_id = m.turn_id AND t.fencing_token <> m.fencing_token
), released AS (
  -- The thread's resume context rides the same update that releases the run,
  -- since one statement may change a row once.
  UPDATE thread.threads s
  SET run_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
      resume_context = $37::jsonb,
      hold_expires_at = CASE WHEN m.run_expires_at > m.checked_at AND m.slot_expires_at > m.checked_at
                             AND m.hold_authorized
                             AND $38::timestamptz > clock_timestamp()
                             THEN LEAST($38::timestamptz, clock_timestamp() + $39::interval) END
  FROM moved m
  WHERE s.fleet_id = m.fleet_id AND s.fencing_token = m.fencing_token
  RETURNING s.hold_expires_at
), owed AS (
  INSERT INTO thread.deliveries
    (id, fleet_id, workspace_id, provider, destination, turn_id, answer,
     receipt, claim_expires_at, delivered_at, attempt_count, created_at, updated_at)
  -- A run that wrote nothing owes nothing, unless it failed: then its thread
  -- is owed the notice, so a person who asked is not left reading silence.
  SELECT $40, m.fleet_id, m.workspace_id, d.reply_provider, d.reply_address, m.turn_id,
         COALESCE(NULLIF($29::text, ''), $51::text), $41, clock_timestamp(), NULL, 0, now(), now()
  FROM moved m
  CROSS JOIN settled d
  WHERE d.reply_provider = ANY($42::text[])
    AND COALESCE(NULLIF($29::text, ''), $51::text) IS NOT NULL
  ON CONFLICT ON CONSTRAINT uq_deliveries_fleet_id_turn_id DO NOTHING
  RETURNING id
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $43, $2, $44::text, $45::jsonb, NULL, now() FROM moved
)
SELECT
    (SELECT count(*) FROM moved)::bigint AS claimed,
    (SELECT hold_expires_at FROM released) AS accepted_hold,
    -- A locking read answers the latest committed version, so a copy that
    -- waited behind the one that settled sees it settled, not its snapshot.
    (SELECT l.status FROM thread.runs l WHERE l.id = $1 AND l.runner_id = $2 FOR SHARE)
        AS run_status,
    p.posture AS held_posture, p.provider AS held_provider, p.model AS held_model,
    (SELECT resource_version FROM catalogue) AS catalogue_version,
    p.fencing_token >= p.live_fence AS fence_holds,
    p.task_id IS NOT NULL AS has_task,
    (SELECT reply_provider FROM settled) AS reply_provider,
    (SELECT id FROM owed) AS owed_id,
    -- The run's token sums, this statement's lines included. Summed as
    -- numeric: a sum past a bigint answers NULL, and the caller records no
    -- tokens rather than failing a report whose money already settled.
    CASE WHEN tk.input_tokens <= $50::bigint THEN tk.input_tokens::bigint END
        AS input_tokens,
    CASE WHEN tk.cached_input_tokens <= $50::bigint THEN tk.cached_input_tokens::bigint END
        AS cached_input_tokens,
    CASE WHEN tk.cache_write_tokens <= $50::bigint THEN tk.cache_write_tokens::bigint END
        AS cache_write_tokens,
    CASE WHEN tk.output_tokens <= $50::bigint THEN tk.output_tokens::bigint END
        AS output_tokens,
    ad.nanos AS charged_nanos, ad.model_nanos,
    -- The closing row, as `update_turn_result.sql` answered it.
    c.fleet_id, c.turn_id, c.workspace_id, c.actor, c.event_type, c.status, c.token_count,
    c.wall_ms, c.failure_reason, c.failure_detail, c.resumes_turn_id, c.created_at,
    c.updated_at,
    -- NULL only when the event has no line at all, as before.
    CASE WHEN ad.lines = 0 AND NOT EXISTS (
              SELECT 1 FROM billing.usage_lines charge
              WHERE charge.turn_id = c.turn_id AND charge.fleet_id = c.fleet_id)
         THEN NULL
         ELSE COALESCE((SELECT SUM(charge.charged_nanos) FROM billing.usage_lines charge
                         WHERE charge.turn_id = c.turn_id AND charge.fleet_id = c.fleet_id), 0)::bigint
              + ad.nanos
    END AS cost_nanos,
    f.status AS fleet_status,
    (SELECT COUNT(*) FROM fleet.approval_gates g
      WHERE g.fleet_id = c.fleet_id AND g.status = $36) AS pending_approvals,
    COALESCE((SELECT k.event_count FROM fleet.activity_counters k
               WHERE k.fleet_id = c.fleet_id), 0) AS event_count,
    COALESCE((SELECT k.budget_used_nanos FROM fleet.activity_counters k
               WHERE k.fleet_id = c.fleet_id), 0) + ad.nanos AS budget_used_nanos
FROM (SELECT 1) AS one
CROSS JOIN added ad
CROSS JOIN LATERAL (
    SELECT COALESCE(sum(u.input_token_count), 0)::numeric + ad.input_tokens AS input_tokens,
           COALESCE(sum(u.cached_input_token_count), 0)::numeric + ad.cached_input_tokens
               AS cached_input_tokens,
           COALESCE(sum(u.cache_write_token_count::numeric + u.cache_write_5m_token_count
                        + u.cache_write_1h_token_count), 0) + ad.cache_write_tokens
               AS cache_write_tokens,
           COALESCE(sum(u.output_token_count), 0)::numeric + ad.output_tokens AS output_tokens
    FROM billing.usage_lines u
    WHERE u.work_type = $7 AND u.work_id = $1
) tk
LEFT JOIN probe p ON true
LEFT JOIN closed c ON true
LEFT JOIN fleet.fleets f ON f.id = c.fleet_id

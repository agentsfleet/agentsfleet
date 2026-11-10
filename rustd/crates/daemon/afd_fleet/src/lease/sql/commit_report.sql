-- A terminal report in one statement (M245 §6): every write the report's
-- transaction made, under the lease's own row lock and no host lock.
--
-- Lock order is the report's and the renewal's: the fleet's scope, then the
-- slot, then the lease. READ COMMITTED re-reads a locked row's latest version,
-- so the lease's cursor is current, and the runtime line is numbered by its
-- slice's start: a racing renewal waiting on the same lock bills a later slice
-- under a later number. A common table expression cannot see its own statement's
-- writes, nor the triggers' (`trg_usage_lines_bump_budget`), so the closing
-- row's cost and counters and the run's token sums add the lines inserted here.
--
-- Guarded on the posture, provider, model and catalogue version the lines were
-- priced at: a wrong guess writes nothing, and the final row names the lease's
-- own facts for the caller's one retry. When the guard fails, the row says
-- whether this runner already settled the lease or lost its fence.
--
-- $1 lease · $2 runner · $3 milliseconds per second · $4 active status ·
-- $5 reported status · $6 succeeded · $7 the lease work type ·
-- $8 the model line kind · $9 run nanos per second ·
-- $10 the runtime line's price · $11 posture · $12 provider · $13 model ·
-- $14 the catalogue version the lines were priced at ·
-- $15-$26 the runner's request lines, in parallel arrays: id, seq, requested
-- at, input, cached input, cache write, 5-minute write, 1-hour write, output,
-- price, unpriced tokens, charged · $27 the runtime line's id ·
-- $28 the event's status · $29 response · $30 tokens · $31 wall time ·
-- $32 failure label · $33 failure detail · $34 tool calls · $35 the status an
-- open event holds · $36 the pending gate status · $37 the session document ·
-- $38 the hold the runner asked for · $39 the hold's idle cap ·
-- $40 the obligation's id · $41 its receipt · $42 the answer owed ·
-- $43 the connectors an answer can be owed to · $44 the audit row's id ·
-- $45 its event type · $46 its metadata · $47 the fleet · $48 its workspace ·
-- $49 the most milliseconds one line carries · $50 the runtime line kind
WITH scope AS MATERIALIZED (
  SELECT f.id
  FROM fleet.fleets f
  JOIN identity.lock_workspace($48) w ON w.id = f.workspace_id
  WHERE f.id = $47
  FOR KEY SHARE OF f
), locked_slot AS MATERIALIZED (
  SELECT s.fleet_id, s.fencing_token
  FROM runner.fleet_slots s
  JOIN scope sc ON sc.id = s.fleet_id
  JOIN runner.leases l ON l.fleet_id = s.fleet_id
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR UPDATE OF s
), probe AS MATERIALIZED (
  SELECT l.id, l.fleet_id, l.workspace_id, l.tenant_id, l.event_id, l.fencing_token,
         l.created_at, l.lease_expires_at, l.last_metered_at,
         l.posture, l.provider, l.model, l.task_id, a.fencing_token AS live_fence
  FROM runner.leases l
  JOIN locked_slot a ON a.fleet_id = l.fleet_id
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR UPDATE OF l
), catalogue AS MATERIALIZED (
  SELECT c.resource_version FROM model.catalogue_version c WHERE c.id = 1
), guard AS MATERIALIZED (
  SELECT p.*, GREATEST(p.last_metered_at, LEAST(i.checked_at, p.lease_expires_at)) AS metered_until
  FROM probe p
  CROSS JOIN (SELECT clock_timestamp() AS checked_at) i
  WHERE p.fencing_token >= p.live_fence
    AND p.posture = $11 AND p.provider = $12 AND p.model = $13
    AND ($14::bigint IS NULL OR (SELECT resource_version FROM catalogue) = $14::bigint)
    AND EXISTS (
        SELECT 1 FROM fleet.admissions a
        WHERE a.id = p.event_id AND a.fleet_id = p.fleet_id
          AND a.lease_fence = p.fencing_token AND a.settled_at IS NULL)
), measured AS MATERIALIZED (
  -- Capped as the rater caps a line's milliseconds ($49).
  SELECT g.*,
         LEAST((EXTRACT(EPOCH FROM g.metered_until - g.last_metered_at) * $3::bigint)::bigint,
               $49::bigint) AS d_ms
  FROM guard g
), runtime AS MATERIALIZED (
  -- Charged as the rater charges run time: whole nanos, rounded up, in integers.
  -- Numbered by the slice's start, in milliseconds from the lease's start.
  SELECT (EXTRACT(EPOCH FROM m.last_metered_at - m.created_at) * $3::bigint)::bigint AS seq,
         m.metered_until AS requested_at,
         m.d_ms AS runtime_ms,
         (m.d_ms * $9::bigint + $3::bigint - 1) / $3::bigint AS charged
  FROM measured m
  WHERE m.d_ms > 0
), lines AS (
  INSERT INTO billing.usage_lines
      (id, work_type, work_id, kind, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
       authoring_attempt_id, provider, model, posture, requested_at, input_token_count,
       cached_input_token_count, cache_write_token_count, cache_write_5m_token_count,
       cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id,
       unpriced_token_count, charged_nanos, created_at, updated_at)
  SELECT line.id, $7, m.id, line.kind, line.seq, m.tenant_id, m.workspace_id, m.fleet_id, f.name, m.event_id,
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
      SELECT $27::uuid, $50::text, r.seq, r.requested_at, 0, 0, 0, 0, 0, 0, r.runtime_ms, $10::uuid, 0,
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
  UPDATE runner.leases l
  SET status = $5,
      last_metered_at = m.metered_until,
      updated_at = now()
  FROM measured m
  WHERE l.id = m.id
  RETURNING l.id, l.fleet_id, l.event_id, l.fencing_token, l.workspace_id
), settled AS (
  UPDATE fleet.admissions a
  SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
  FROM moved m
  WHERE a.id = m.event_id AND a.fleet_id = m.fleet_id
    AND a.lease_fence = m.fencing_token AND a.settled_at IS NULL
  RETURNING a.reply_provider, a.reply_address
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
  UPDATE fleet.events e
  SET status = $28, response_text = $29, token_count = $30, wall_ms = $31, updated_at = now(),
      failure_reason = $32, failure_detail = $33, tool_calls = $34::jsonb
  FROM moved m
  WHERE e.fleet_id = m.fleet_id AND e.event_id = m.event_id AND e.status = $35
  RETURNING e.fleet_id, e.event_id, e.workspace_id, e.actor, e.event_type, e.status,
            e.token_count, e.wall_ms, e.failure_reason, e.failure_detail,
            e.resumes_event_id, e.created_at, e.updated_at
), dropped AS (
  -- A reclaimed lease's records are of a run whose answer does not stand.
  DELETE FROM fleet.tool_calls t
  USING moved m
  WHERE t.fleet_id = m.fleet_id AND t.event_id = m.event_id AND t.fencing_token <> m.fencing_token
), checkpointed AS (
  INSERT INTO fleet.sessions (fleet_id, context_json, created_at, updated_at)
  SELECT m.fleet_id, $37::jsonb, now(), now() FROM moved m
  ON CONFLICT (fleet_id) DO UPDATE
    SET context_json = EXCLUDED.context_json, updated_at = now()
), released AS (
  UPDATE runner.fleet_slots s
  SET lease_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
      hold_expires_at = CASE WHEN $38::timestamptz > clock_timestamp()
                             THEN LEAST($38::timestamptz, clock_timestamp() + $39::interval) END
  FROM moved m
  WHERE s.fleet_id = m.fleet_id AND s.fencing_token = m.fencing_token
  RETURNING s.hold_expires_at
), owed AS (
  INSERT INTO fleet.obligations
    (id, fleet_id, workspace_id, provider, destination, event_id, answer,
     receipt, claim_expires_at, delivered_at, attempt_count, created_at, updated_at)
  SELECT $40, m.fleet_id, m.workspace_id, d.reply_provider, d.reply_address, m.event_id,
         $42::text, $41, clock_timestamp(), NULL, 0, now(), now()
  FROM moved m
  CROSS JOIN settled d
  WHERE d.reply_provider = ANY($43::text[]) AND $42::text <> ''
  ON CONFLICT ON CONSTRAINT uq_obligations_fleet_id_event_id DO NOTHING
  RETURNING id
), audit AS (
  INSERT INTO runner.events (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $44, $2, $45::text, $46::jsonb, NULL, now() FROM moved
  RETURNING id
)
SELECT
    (SELECT count(*) FROM moved)::bigint AS claimed,
    -- A locking read answers the latest committed version, so a copy that
    -- waited behind the one that settled sees it settled, not its snapshot.
    (SELECT l.status FROM runner.leases l WHERE l.id = $1 AND l.runner_id = $2 FOR SHARE)
        AS lease_status,
    p.posture AS held_posture, p.provider AS held_provider, p.model AS held_model,
    (SELECT resource_version FROM catalogue) AS catalogue_version,
    p.fencing_token >= p.live_fence AS fence_holds,
    p.task_id IS NOT NULL AS has_task,
    (SELECT reply_provider FROM settled) AS reply_provider,
    (SELECT id FROM owed) AS owed_id,
    (SELECT id FROM audit) AS audit_id,
    (SELECT hold_expires_at FROM released) AS hold_expires_at,
    -- The run's token sums, this statement's lines included. Summed as
    -- numeric: a sum past a bigint answers NULL, and the caller records no
    -- tokens rather than failing a report whose money already settled.
    CASE WHEN tk.input_tokens <= 9223372036854775807 THEN tk.input_tokens::bigint END
        AS input_tokens,
    CASE WHEN tk.cached_input_tokens <= 9223372036854775807 THEN tk.cached_input_tokens::bigint END
        AS cached_input_tokens,
    CASE WHEN tk.cache_write_tokens <= 9223372036854775807 THEN tk.cache_write_tokens::bigint END
        AS cache_write_tokens,
    CASE WHEN tk.output_tokens <= 9223372036854775807 THEN tk.output_tokens::bigint END
        AS output_tokens,
    ad.nanos AS charged_nanos, ad.model_nanos,
    -- The closing row, as `update_fleet_event_result.sql` answered it.
    c.fleet_id, c.event_id, c.workspace_id, c.actor, c.event_type, c.status, c.token_count,
    c.wall_ms, c.failure_reason, c.failure_detail, c.resumes_event_id, c.created_at,
    c.updated_at,
    -- NULL only when the event has no line at all, as before.
    CASE WHEN ad.lines = 0 AND NOT EXISTS (
              SELECT 1 FROM billing.usage_lines charge
              WHERE charge.event_id = c.event_id AND charge.fleet_id = c.fleet_id)
         THEN NULL
         ELSE COALESCE((SELECT SUM(charge.charged_nanos) FROM billing.usage_lines charge
                         WHERE charge.event_id = c.event_id AND charge.fleet_id = c.fleet_id), 0)::bigint
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

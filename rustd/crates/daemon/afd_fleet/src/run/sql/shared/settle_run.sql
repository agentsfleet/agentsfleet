-- The settlement both kinds share: the run flipped to its reported status at its last
-- metered instant, its thread freed with any hold the runner asked for, and an audit row,
-- under the thread and run locks. The caller has locked the owner's scope
-- first, which the fleet's report statement (`commit_report.sql`) does in its own first
-- step; the final slice's usage line and the owner's own rows are its own statements, in
-- the same transaction. A run whose fence moved on writes nothing, and the row says so.
--
-- $1 run · $2 runner · $3 milliseconds per second · $4 active status · $5 settled status ·
-- $6 the hold the runner asked for, NULL for none · $7 the hold's idle cap ·
-- $8 audit row · $9 its event type · $10 its metadata ·
-- $11 the longest runtime one line may claim · $12-$13 the opening's credential proof and
-- connection, NULL for a trusted call · $14 the host states that may keep a hold
WITH hold_host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r
  WHERE r.id = $2
    AND $6::timestamptz IS NOT NULL
    AND r.degraded_reason IS NULL AND r.status = ANY($14::text[])
    AND ($12::text IS NULL
         OR (r.token_hash = $12::text AND r.connection_id IS NOT DISTINCT FROM $13::uuid))
  FOR SHARE OF r
), locked_thread AS MATERIALIZED (
  SELECT s.id, s.fencing_token, s.run_expires_at AS thread_expires_at,
         h.id IS NOT NULL AS hold_authorized
  FROM thread.threads s
  JOIN thread.runs l ON s.id = COALESCE(l.fleet_id, l.draft_id)
  LEFT JOIN hold_host h ON h.id = l.runner_id
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR NO KEY UPDATE OF s
), probe AS MATERIALIZED (
  SELECT l.id, l.workspace_id, l.tenant_id, l.turn_id, l.fencing_token, l.created_at,
         l.run_expires_at, l.last_metered_at,
         s.id AS thread_id, s.fencing_token AS live_fence, s.thread_expires_at, s.hold_authorized
  FROM thread.runs l
  JOIN locked_thread s ON s.id = COALESCE(l.fleet_id, l.draft_id)
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the thread and run locks are held, so the slice is measured to the moment
  -- the settlement owns the run, never before a lock wait.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM probe) locked
), measured AS MATERIALIZED (
  SELECT p.*, i.checked_at,
         GREATEST(p.last_metered_at, LEAST(i.checked_at, p.run_expires_at)) AS metered_until
  FROM probe p
  CROSS JOIN instant i
  WHERE p.fencing_token >= p.live_fence
), moved AS (
  UPDATE thread.runs l
  SET status = $5,
      last_metered_at = m.metered_until,
      -- The run ended at its settlement. Its cursor then meets its deadline, so it
      -- leaves the metering sweep's partial index.
      run_expires_at = m.metered_until,
      updated_at = now()
  FROM measured m
  WHERE l.id = m.id
  RETURNING l.id, l.fencing_token, m.thread_id, m.run_expires_at, m.thread_expires_at,
            m.hold_authorized, m.checked_at
), released AS (
  UPDATE thread.threads s
  SET run_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
      hold_expires_at = CASE WHEN m.run_expires_at > m.checked_at
                             AND m.thread_expires_at > m.checked_at
                             AND m.hold_authorized
                             AND $6::timestamptz > clock_timestamp()
                             THEN LEAST($6::timestamptz, clock_timestamp() + $7::interval) END
  FROM moved m
  WHERE s.id = m.thread_id AND s.fencing_token = m.fencing_token
  RETURNING s.hold_expires_at
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $8, $2, $9::text, $10::jsonb, NULL, now() FROM moved
)
SELECT
    (SELECT count(*) FROM moved)::bigint AS moved_runs,
    (SELECT hold_expires_at FROM released) AS accepted_hold,
    -- A locking read answers the latest committed version, so a copy that waited behind
    -- the one that settled sees it settled, not its snapshot.
    (SELECT l.status FROM thread.runs l WHERE l.id = $1 AND l.runner_id = $2 FOR SHARE)
        AS run_status,
    p.fencing_token >= p.live_fence AS fence_holds,
    m.metered_until AS checked_at, m.created_at AS started_at,
    -- Capped at the longest runtime one line may claim, which is where the rater caps it.
    LEAST((EXTRACT(EPOCH FROM m.metered_until - m.last_metered_at) * $3::bigint)::bigint,
          $11::bigint) AS d_ms,
    -- The runtime line's number: the milliseconds from the run's start to the slice's.
    (EXTRACT(EPOCH FROM m.last_metered_at - m.created_at) * $3::bigint)::bigint AS runtime_seq,
    p.tenant_id, p.workspace_id, p.turn_id
FROM (SELECT 1) AS one
LEFT JOIN probe p ON true
LEFT JOIN measured m ON true

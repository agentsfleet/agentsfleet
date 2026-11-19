-- The settlement both kinds share: the run flipped to its reported status at its last
-- metered instant, its slot freed with any hold the runner asked for, the runner's tally and
-- an audit row, under the slot and run locks. The caller has locked the owner's scope
-- first, which the fleet's report statement (`commit_report.sql`) does in its own first
-- step; the final slice's usage line and the owner's own rows are its own statements, in
-- the same transaction. A run whose fence moved on writes nothing, and the row says so.
--
-- $1 run · $2 runner · $3 milliseconds per second · $4 active status · $5 reported status ·
-- $6 succeeded · $7 the hold the runner asked for, NULL for none · $8 the hold's idle cap ·
-- $9 audit row · $10 its event type · $11 its metadata ·
-- $12 the longest runtime one line may claim · $13-$14 the opening's credential proof and
-- connection, NULL for a trusted call · $15 the host states that may keep a hold
WITH hold_host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r
  WHERE r.id = $2
    AND $7::timestamptz IS NOT NULL
    AND r.degraded_reason IS NULL AND r.status = ANY($15::text[])
    AND ($13::text IS NULL
         OR (r.token_hash = $13::text AND r.connection_id IS NOT DISTINCT FROM $14::uuid))
  FOR SHARE OF r
), locked_slot AS MATERIALIZED (
  SELECT s.id, s.fencing_token, s.run_expires_at AS slot_expires_at,
         h.id IS NOT NULL AS hold_authorized
  FROM thread.threads s
  JOIN thread.runs l ON s.id = COALESCE(l.fleet_id, l.draft_id)
  LEFT JOIN hold_host h ON h.id = l.runner_id
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR UPDATE OF s
), probe AS MATERIALIZED (
  SELECT l.id, l.workspace_id, l.tenant_id, l.event_id, l.fencing_token, l.created_at,
         l.run_expires_at, l.last_metered_at,
         s.id AS slot_id, s.fencing_token AS live_fence, s.slot_expires_at, s.hold_authorized
  FROM thread.runs l
  JOIN locked_slot s ON s.id = COALESCE(l.fleet_id, l.draft_id)
  WHERE l.id = $1 AND l.runner_id = $2 AND l.status = $4
  FOR UPDATE OF l
), instant AS MATERIALIZED (
  -- Read once the slot and run locks are held, so the slice is measured to the moment
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
  RETURNING l.id, l.fencing_token, m.slot_id, m.run_expires_at, m.slot_expires_at,
            m.hold_authorized, m.checked_at
), tally AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT $2, 0, CASE WHEN $6::boolean THEN 1 ELSE 0 END,
         CASE WHEN $6::boolean THEN 0 ELSE 1 END, 0, now(), now()
  FROM moved
  ON CONFLICT (runner_id) DO UPDATE
     SET succeeded_count = c.succeeded_count + EXCLUDED.succeeded_count,
         failed_count    = c.failed_count + EXCLUDED.failed_count,
         updated_at      = now()
), released AS (
  UPDATE thread.threads s
  SET run_expires_at = clock_timestamp() - interval '1 millisecond', updated_at = now(),
      hold_expires_at = CASE WHEN m.run_expires_at > m.checked_at
                             AND m.slot_expires_at > m.checked_at
                             AND m.hold_authorized
                             AND $7::timestamptz > clock_timestamp()
                             THEN LEAST($7::timestamptz, clock_timestamp() + $8::interval) END
  FROM moved m
  WHERE s.id = m.slot_id AND s.fencing_token = m.fencing_token
  RETURNING s.hold_expires_at
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $9, $2, $10::text, $11::jsonb, NULL, now() FROM moved
)
SELECT
    (SELECT count(*) FROM moved)::bigint AS claimed,
    (SELECT hold_expires_at FROM released) AS accepted_hold,
    -- A locking read answers the latest committed version, so a copy that waited behind
    -- the one that settled sees it settled, not its snapshot.
    (SELECT l.status FROM thread.runs l WHERE l.id = $1 AND l.runner_id = $2 FOR SHARE)
        AS run_status,
    p.fencing_token >= p.live_fence AS fence_holds,
    m.metered_until AS checked_at, m.created_at AS started_at,
    -- Capped at the longest runtime one line may claim, which is where the rater caps it.
    LEAST((EXTRACT(EPOCH FROM m.metered_until - m.last_metered_at) * $3::bigint)::bigint,
          $12::bigint) AS d_ms,
    -- The runtime line's number: the milliseconds from the run's start to the slice's.
    (EXTRACT(EPOCH FROM m.last_metered_at - m.created_at) * $3::bigint)::bigint AS runtime_seq,
    p.tenant_id, p.workspace_id, p.event_id
FROM (SELECT 1) AS one
LEFT JOIN probe p ON true
LEFT JOIN measured m ON true

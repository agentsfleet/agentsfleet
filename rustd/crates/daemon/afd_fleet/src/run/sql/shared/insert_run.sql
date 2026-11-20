-- The issue both kinds share: the run, the host's placement choice, its audit row and the
-- runner's tally, fenced on the assignment that earned it. The caller holds the host row
-- (`lock_host.sql`) through this transaction, so this statement's snapshot, taken after that
-- lock, sees every earlier assignment's commit: the census below counts them. The caller has
-- proved and locked the owner's scope earlier in the transaction; the fleet's own insert
-- (`insert_run_with_turn.sql`) proves it in its insert instead. A full host, a moved thread
-- fence or a moved placement choice writes nothing that commits.
--
-- $1 run · $2 runner · $3 the owning fleet, NULL for a draft · $4 the owning draft, NULL
-- for a fleet · $5 workspace · $6 tenant · $7 the event or draft message the run runs ·
-- $8 its producer's instant · $9 posture · $10 provider · $11 model ·
-- $12 the assignment's fence ·
-- $13 active status · $14 audit row · $15 its event type · $16 its metadata ·
-- $17 decision snapshot, NULL for none · $18 the deadline the assignment answered ·
-- $19 the host's run version as read · $20 the run type chosen ·
-- $21 the most seconds the run may run
WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r CROSS JOIN instant i
  WHERE r.id = $2
    AND (SELECT count(*) FROM thread.runs l
          WHERE l.runner_id = $2 AND l.status = $13
            AND l.run_expires_at > i.checked_at)
      -- Every other owner's live hold on this host, fleet or draft.
      + (SELECT count(*) FROM thread.threads s
          WHERE s.last_runner_id = $2 AND s.id <> COALESCE($3::uuid, $4::uuid)
            AND s.hold_expires_at IS NOT NULL
            AND s.hold_expires_at > i.checked_at)
      < r.worker_count
), held AS (
  UPDATE thread.threads
  SET hold_expires_at = NULL, updated_at = now()
  WHERE id = COALESCE($3::uuid, $4::uuid) AND fencing_token = $12
    AND run_expires_at > (SELECT checked_at FROM instant)
    -- The deadline the assignment answered, which a pass that outlived it misses.
    AND $18 > (SELECT checked_at FROM instant)
    AND EXISTS (SELECT 1 FROM host)
  RETURNING run_expires_at
), after_lock AS MATERIALIZED (
  -- Read once `held` has the thread lock, so a wait on that lock cannot make a
  -- lapsed thread or an assignment whose deadline passed look live.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM held) locked
), inserted AS (
  -- The run and its metering cursor start at the clock read after the thread lock:
  -- stamping the earlier instant would bill the lock wait as runtime.
  INSERT INTO thread.runs
  (id, runner_id, fleet_id, draft_id, workspace_id, tenant_id, turn_id, turn_created_at,
   posture, provider, model, last_metered_at, max_runtime_seconds,
   fencing_token, run_expires_at, status, posted_message_count, decision_snapshot,
   created_at, updated_at)
  SELECT $1, $2, $3::uuid, $4::uuid, $5, $6, $7, $8,
         $9, $10, $11, a.checked_at, $21,
         $12, h.run_expires_at, $13, 0, $17, a.checked_at, a.checked_at
  FROM held h
  CROSS JOIN after_lock a
  WHERE h.run_expires_at > a.checked_at
    AND $18 > a.checked_at
  RETURNING id, runner_id, created_at
), chosen AS (
  -- The placement choice the host lock read: the next assignment prefers the other kind.
  UPDATE runner.runners
  SET run_version = run_version + 1, last_run_type = $20::text, updated_at = now()
  WHERE id = $2 AND run_version = $19::bigint AND EXISTS (SELECT 1 FROM inserted)
  RETURNING id
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $14, i.runner_id, $15, $16, NULL, now()
  FROM inserted i
  RETURNING id
), tallied AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT runner_id, 1, 0, 0, 0, now(), now()
  FROM inserted
  ON CONFLICT (runner_id) DO UPDATE
     SET acquired_count = c.acquired_count + 1, updated_at = now()
  RETURNING runner_id
)
-- The stored TIMESTAMPTZ(3) instant. No row when the fence or the choice moved: the caller
-- rolls the run back with it.
SELECT created_at FROM inserted
WHERE EXISTS (SELECT 1 FROM tallied) AND EXISTS (SELECT 1 FROM chosen)

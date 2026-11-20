-- The fleet's issue: `../shared/insert_run.sql` with the fleet's scope and setup proof
-- inside the insert, so the issue stays one statement. A change to the shared bookkeeping
-- belongs in both.
--
-- The caller holds the host row (`lock_host.sql`) through this transaction, so
-- this statement's snapshot, taken after that lock, sees every earlier
-- claimant's commit: the census below counts them. A full host, a moved slot
-- fence, a moved placement choice or a moved setup writes nothing.
WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r CROSS JOIN instant i
  WHERE r.id = $2
    AND (SELECT count(*) FROM thread.runs l
          WHERE l.runner_id = $2 AND l.status = $11
            AND l.run_expires_at > i.checked_at)
      -- Setup turns the authoring path still runs; this term goes with setup.attempts.
      + (SELECT count(*) FROM setup.attempts a
          WHERE a.runner_id = $2 AND a.status = $11
            AND a.run_expires_at > i.checked_at)
      -- Every other owner's live hold on this host, fleet or draft: a slot's id is its owner's.
      + (SELECT count(*) FROM thread.threads s
          WHERE s.last_runner_id = $2 AND s.id <> $3
            AND s.hold_expires_at IS NOT NULL
            AND s.hold_expires_at > i.checked_at)
      < r.worker_count
), held AS (
  UPDATE thread.threads
  SET hold_expires_at = NULL, updated_at = now()
  WHERE fleet_id = $3 AND fencing_token = $10
    AND run_expires_at > (SELECT checked_at FROM instant)
    -- The deadline the claim answered, which a pass that outlived it misses.
    AND $19 > (SELECT checked_at FROM instant)
    AND EXISTS (SELECT 1 FROM host)
  RETURNING fleet_id, run_expires_at
), after_lock AS MATERIALIZED (
  -- Read once `held` has the slot lock, so a wait on that lock cannot make a
  -- lapsed slot or a claim whose deadline passed look live.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM held) locked
), inserted AS (
  -- The run starts, and its metering cursor with it, at the clock read after the
  -- slot lock: stamping the earlier instant would bill the lock wait as runtime.
  INSERT INTO thread.runs
  (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at,
   posture, provider, model, last_metered_at, max_runtime_seconds,
   fencing_token, run_expires_at, status, posted_message_count, decision_snapshot, setup_revision_id, created_at, updated_at)
  SELECT $1, $2, $3, $4, $5, e.id, $15,
         $7, $8, $9, a.checked_at, $23,
         $10, h.run_expires_at, $11, 0, $16, $20::uuid, a.checked_at, a.checked_at
  FROM held h
  CROSS JOIN after_lock a
  JOIN thread.turns e ON e.thread_id = $3 AND e.id = $6
  -- The scope proof thread.runs holds no foreign key for: a workspace or
  -- tenant that is not the fleet's writes no run, so the caller issues none.
  JOIN fleet.fleets f ON f.id = $3 AND f.workspace_id = $4 AND f.tenant_id = $5
  -- The setup the claim assembled is still the head, and a setup that needs
  -- durable tasks still has a host that runs them: what `issue_source.sql`
  -- rechecked for every run, now in the insert so a plain chat reads nothing
  -- more. The issue holds the fleet (`task::run::parents`) and the host, so
  -- neither moves before commit.
  JOIN runner.runners r ON r.id = $2
  LEFT JOIN fleet.revisions setup ON setup.id = f.head_revision_id AND setup.fleet_id = f.id
  -- Recheck after held acquired the slot lock: the slot's own deadline, and the
  -- one the claim answered, which a pass that outlived it misses.
  WHERE h.run_expires_at > a.checked_at
    AND $19 > a.checked_at
    AND ($20::uuid IS NULL OR f.head_revision_id = $20::uuid)
    AND ((r.durable_tasks_enabled
          AND COALESCE(r.capability_report @> jsonb_build_object($21::text, true), false))
         OR setup.definition #> $22::text[] IS NULL
         OR setup.definition #> $22::text[] = 'null'::jsonb)
  RETURNING id, runner_id, fleet_id, turn_id, created_at
), chosen AS (
  -- The placement choice the host lock read: the next assignment prefers the other kind.
  UPDATE runner.runners
  SET run_version = run_version + 1, last_run_type = $18::text, updated_at = now()
  WHERE id = $2 AND run_version = $17::bigint AND EXISTS (SELECT 1 FROM inserted)
  RETURNING id
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $12, i.runner_id, $13, $14,
         NULL, now()
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
-- Use the value stored by TIMESTAMPTZ(3), not a separately normalized cache timestamp.
-- No row when the choice moved: the caller rolls the run back with it.
SELECT created_at FROM inserted
WHERE EXISTS (SELECT 1 FROM tallied) AND EXISTS (SELECT 1 FROM chosen)

-- The fleet's issue: the run, under its assignment's fence, with the fleet's scope proof inside
-- the insert, so the issue stays one statement.
--
-- The caller holds the host row (`lock_host.sql`) through this transaction, so
-- this statement's snapshot, taken after that lock, sees every earlier
-- assignment's commit: the census below counts them. A full host, a moved thread
-- fence, a moved placement choice or a stopped head writes nothing.
--
-- The census counts live runs only, so it admits what the runner's credits
-- offer. A sandbox hold takes no worker: the runner keeps its holds below its
-- free workers and drops one when a run starts, and the heartbeat clears that
-- hold only a beat later, so counting holds refused a worker the runner had free.
WITH instant AS MATERIALIZED (
  SELECT clock_timestamp() AS checked_at
), host AS MATERIALIZED (
  SELECT r.id
  FROM runner.runners r CROSS JOIN instant i
  WHERE r.id = $2
    AND (SELECT count(*) FROM thread.runs l
          WHERE l.runner_id = $2 AND l.status = $11
            AND l.run_expires_at > i.checked_at)
      < r.worker_count
), held AS (
  UPDATE thread.threads
  SET hold_expires_at = NULL, updated_at = now()
  WHERE fleet_id = $3 AND fencing_token = $10
    AND run_expires_at > (SELECT checked_at FROM instant)
    -- The deadline the assignment answered, which a pass that outlived it misses.
    AND $18 > (SELECT checked_at FROM instant)
    AND EXISTS (SELECT 1 FROM host)
  RETURNING fleet_id, run_expires_at
), after_lock AS MATERIALIZED (
  -- Read once `held` has the thread lock, so a wait on that lock cannot make a
  -- lapsed thread or an assignment whose deadline passed look live.
  SELECT clock_timestamp() AS checked_at FROM (SELECT count(*) FROM held) locked
), head AS MATERIALIZED (
  -- The head, locked once `held` has the thread, the order Stop takes them in. A Stop
  -- that settled it before this statement began, or while `held` waited, issues nothing:
  -- a locking read follows a version committed after this statement's snapshot and
  -- re-checks both predicates on it, where a plain read would keep the snapshot's. FOR
  -- SHARE because it conflicts with an update still in flight, so that update is waited
  -- for and re-checked too; FOR KEY SHARE would read past it.
  SELECT e.id
  FROM thread.turns e
  WHERE e.thread_id = $3 AND e.id = $6
    AND e.settled_at IS NULL AND e.run_fence = $10
    -- `held` returned the thread, so this runs after its wait and never when it refused.
    AND EXISTS (SELECT 1 FROM held)
  FOR SHARE OF e
), inserted AS (
  -- The run starts, and its metering cursor with it, at the clock read after the
  -- thread lock: stamping the earlier instant would bill the lock wait as runtime.
  INSERT INTO thread.runs
  (id, runner_id, fleet_id, workspace_id, tenant_id, turn_id, turn_created_at,
   posture, provider, model, last_metered_at,
   fencing_token, run_expires_at, status, decision_snapshot, created_at, updated_at)
  SELECT $1, $2, $3, $4, $5, e.id, $15,
         $7, $8, $9, a.checked_at,
         $10, h.run_expires_at, $11, $16, a.checked_at, a.checked_at
  FROM held h
  CROSS JOIN after_lock a
  CROSS JOIN head e
  -- The scope proof thread.runs holds no foreign key for: a workspace or
  -- tenant that is not the fleet's writes no run, so the caller issues none.
  JOIN fleet.fleets f ON f.id = $3 AND f.workspace_id = $4 AND f.tenant_id = $5
  -- Recheck after held acquired the thread lock: the thread's own deadline, and the
  -- one the assignment answered, which a pass that outlived it misses.
  WHERE h.run_expires_at > a.checked_at
    AND $18 > a.checked_at
  RETURNING id, runner_id, fleet_id, turn_id, created_at
), chosen AS (
  -- The placement choice the host lock read: an assignment that read an older one writes nothing.
  UPDATE runner.runners
  SET run_version = run_version + 1, updated_at = now()
  WHERE id = $2 AND run_version = $17::bigint AND EXISTS (SELECT 1 FROM inserted)
  RETURNING id
), audit AS (
  INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
  SELECT $12, i.runner_id, $13, $14,
         NULL, now()
  FROM inserted i
  RETURNING id
)
-- Use the value stored by TIMESTAMPTZ(3), not a separately normalized cache timestamp.
-- No row when the choice moved: the caller rolls the run back with it.
SELECT created_at FROM inserted
WHERE EXISTS (SELECT 1 FROM chosen)

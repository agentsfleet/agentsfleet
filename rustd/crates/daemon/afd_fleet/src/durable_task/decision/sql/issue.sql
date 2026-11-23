-- A decision reservation's one locking statement: it issues the call. The reads before it
-- hold nothing; this takes, each for this statement only and in the order every writer of
-- the fleet takes them, the workspace and the fleet (key share), the thread, the run, the
-- named task, the source turn and a retried call (share). A share step that waited re-reads
-- its row and re-checks its predicates on the committed version; the fleet's key share does
-- not wait for an archive, so the fleet's status is read as of the statement's start. The clock is read once, after the
-- locks. The call is inserted as issued with its payload, or, when the operation's call is
-- already saved, nothing is written; no row means a state the reads relied on moved.
--
-- Predicates on rows this only joins (runner, model, revision, the task's person check) are
-- read as of the statement's start too.
--
-- $1 call · $2 runner · $3 run · $4 source turn · $5 fleet · $6 workspace · $7 tenant ·
-- $8 the goal's task (the call's budget owner) · $9 its generation · $10 fence ·
-- $11 operation · $12 request digest · $13 assessment · $14 assessment key · $15 attempt ·
-- $16 retried call · $17 its logical deadline · $18 logical seconds · $19 purpose ·
-- $20 issued status · $21 provider · $22 fleet name · $23 profile snapshot ·
-- $24 price snapshot · $25 context snapshot · $26 reserved nanos · $27 platform model ·
-- $28 decision revision · $29 private request · $30 active run status ·
-- $31 active fleet status · $32 active runner status · $33 durable-task capability ·
-- $34 the run's named task · $35 its generation · $36 running task state ·
-- $37 Slack mention producer · $38 Slack provider · $39 Slack person actor pattern ·
-- $40 its separator · $41 the frozen decision assignment the request was compiled from ·
-- $42 the state a retried call must still be in
WITH workspace AS MATERIALIZED (
  SELECT id FROM identity.lock_workspace($6)
), fleet AS MATERIALIZED (
  SELECT f.id
  FROM fleet.fleets f
  JOIN workspace w ON w.id = f.workspace_id
  WHERE f.id = $5 AND f.tenant_id = $7 AND f.status = $31
  FOR KEY SHARE OF f
), thread AS MATERIALIZED (
  SELECT s.fleet_id, s.run_expires_at
  FROM thread.threads s
  JOIN fleet ON fleet.id = s.fleet_id
  WHERE s.fleet_id = $5 AND s.last_runner_id = $2 AND s.fencing_token = $10
  FOR SHARE OF s
), run AS MATERIALIZED (
  SELECT l.id, l.turn_id, l.run_expires_at, l.durable_task_stop_generation
  FROM thread.runs l
  JOIN thread ON thread.fleet_id = l.fleet_id
  WHERE l.id = $3 AND l.runner_id = $2 AND l.status = $30 AND l.fencing_token = $10
    AND l.decision_snapshot = $41::jsonb
    AND l.durable_task_id IS NOT DISTINCT FROM $34::uuid
  FOR SHARE OF l
), task AS MATERIALIZED (
  -- Only when the run names a task; a run that names none takes no task lock.
  SELECT t.id, t.expires_at
  FROM fleet.durable_tasks t
  JOIN run ON true
  WHERE $34::uuid IS NOT NULL AND t.id = $34 AND t.fleet_id = $5 AND t.workspace_id = $6
    AND t.generation = $35 AND t.stop_generation = run.durable_task_stop_generation
    AND t.status = $36
  FOR SHARE OF t
), source AS MATERIALIZED (
  -- After the task, whatever it found: the run's own turn, or one still assigned to it.
  SELECT a.id
  FROM thread.turns a
  JOIN run ON true
  CROSS JOIN (SELECT count(*) FROM task) ordered
  WHERE a.id = $4 AND a.thread_id = $5 AND a.workspace_id = $6 AND a.settled_at IS NULL
    AND (a.id = run.turn_id OR (a.input_run_id = $3 AND a.input_fence = $10))
  FOR SHARE OF a
), parent AS MATERIALIZED (
  -- Only for a retry, after the source turn: the call it retries is still unknown. A
  -- receipt takes the thread before the call, so it has committed by the time this reads.
  SELECT p.id
  FROM runner.decision_calls p
  CROSS JOIN (SELECT count(*) FROM source) ordered
  WHERE $16::uuid IS NOT NULL AND p.id = $16
    AND p.status = $42 AND p.canonical_receipt_digest IS NULL
  FOR SHARE OF p
), instant AS MATERIALIZED (
  SELECT clock_timestamp() AS at FROM (SELECT count(*) FROM parent) locked
), proved AS MATERIALIZED (
  SELECT i.at
  FROM instant i
  CROSS JOIN thread
  CROSS JOIN run
  CROSS JOIN source
  JOIN runner.runners r ON r.id = $2 AND r.status = $32
  JOIN model.platform_models m ON m.id = $27 AND m.decision_enabled
  JOIN model.decision_revisions d ON d.id = $28 AND d.platform_model_id = m.id
  WHERE thread.run_expires_at > i.at AND run.run_expires_at > i.at
    AND COALESCE(r.capability_report @> jsonb_build_object($33::text, true), false)
    AND ($34::uuid IS NULL OR (
      EXISTS (SELECT 1 FROM task WHERE task.expires_at > i.at)
      AND fleet.durable_task_person_allowed($34, $37, $38, $39, $40)))
    AND ($16::uuid IS NULL OR EXISTS (SELECT 1 FROM parent))
    AND ($17::timestamptz IS NULL OR $17::timestamptz > i.at)
), inserted AS (
  INSERT INTO runner.decision_calls AS c (
      id, original_runner_id, original_run_id, source_turn_id,
      fleet_id, workspace_id, tenant_id, durable_task_id, durable_task_generation,
      fencing_token, operation_id, request_digest, assessment_id, assessment_key,
      attempt_number, retry_of_decision_call_id, logical_expires_at, purpose, status, provider,
      fleet_name, profile_snapshot, price_snapshot, context_snapshot, usage_sequence,
      reserved_nanos, issued_at, created_at, updated_at, platform_model_id, decision_revision_id)
  SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9,
         $10, $11, $12, $13, $14,
         $15, $16, COALESCE($17::timestamptz, p.at + make_interval(secs => $18)), $19, $20, $21,
         $22, $23, $24, $25,
         -- The run's next decision line, from 0: a concurrent statement of the same run that
         -- drew the same number fails `uq_decision_calls_usage`, and its caller runs this again.
         (SELECT COALESCE(MAX(n.usage_sequence) + 1, 0)
            FROM runner.decision_calls n WHERE n.original_run_id = $3),
         $26, p.at, p.at, p.at, $27, $28
  FROM proved p
  -- A call already saved for this operation is left untouched. A no-op update of it
  -- would write a new row version, whose index entries can wait on a concurrent
  -- inserter that is itself waiting on this transaction: a deadlock under a storm.
  ON CONFLICT (original_run_id, operation_id) DO NOTHING
  RETURNING c.id, c.logical_expires_at, c.created_at
), payload AS (
  INSERT INTO runner.decision_payloads (
      decision_call_id, fleet_id, workspace_id, tenant_id, durable_task_id, request,
      created_at, updated_at)
  SELECT i.id, $5, $6, $7, $8, $29, i.created_at, i.created_at
  FROM inserted i
  RETURNING decision_call_id
)
-- One row when every check held: the call this inserted, or a NULL id when the operation's
-- call was already saved, which the caller then reads by its operation. So a lost race
-- never reads as a state that moved.
SELECT i.id, i.logical_expires_at FROM proved p LEFT JOIN inserted i ON true

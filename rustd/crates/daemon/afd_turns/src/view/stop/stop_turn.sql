-- Stop: a person ends one turn of a thread, a fleet's or a setup draft's alike, in one
-- statement. It locks the thread first, as a claim, a renewal and a report do, so it
-- queues behind any of them rather than deadlocking against one.
--
-- An unsettled turn is cancelled and settled, so no claim takes it again. When a live run
-- serves it, the run gets its durable cancel mark, and every turn that run serves is
-- cancelled with it: the turn it took as its head and every message it folded in. A turn
-- a run has not taken stays queued. A waiting turn's open approval closes with it.
--
-- A settled turn is left alone: the row names its status, so the caller answers a
-- cancelled turn with its state and refuses any other. No row: no such turn in this
-- workspace and thread.
--
-- A fleet's turn is opened as it settles, so history shows the stopped message; a setup
-- draft's turn never opens a fleet event.
--
-- $1 workspace · $2 thread · $3 turn · $4 the status Stop writes · $5 the active run
-- status · $6 the status a closed approval takes · $7 who it records as its resolver ·
-- $8 the detail it records
WITH thread AS MATERIALIZED (
  SELECT s.id FROM thread.threads s
  WHERE s.id = $2
  FOR NO KEY UPDATE OF s
), target AS MATERIALIZED (
  SELECT t.id, t.sequence, t.status, t.settled_at IS NOT NULL AS settled, t.run_fence,
         t.input_claim_run_id, t.input_claim_fence
  FROM thread.turns t
  JOIN thread ON thread.id = t.thread_id
  WHERE t.id = $3 AND t.workspace_id = $1
  FOR UPDATE OF t
), run AS MATERIALIZED (
  -- The live run serving the target: the run that took it as its head under the fence it
  -- stamped, or the run whose conversation folded it in. A setup draft's turn carries no
  -- fence, so its run is the one that took it.
  SELECT r.id, r.runner_id, r.turn_id, r.fencing_token
  FROM target t
  JOIN thread.runs r ON COALESCE(r.fleet_id, r.draft_id) = $2
  WHERE NOT t.settled AND r.status = $5
    AND ((r.turn_id = t.id AND r.fencing_token = COALESCE(t.run_fence, r.fencing_token))
         OR (r.id = t.input_claim_run_id AND r.fencing_token = t.input_claim_fence))
  FOR UPDATE OF r
), marked AS (
  UPDATE thread.runs r
  SET cancel_requested_at = COALESCE(r.cancel_requested_at, clock_timestamp()),
      updated_at = now()
  FROM run
  WHERE r.id = run.id
  RETURNING r.id, r.runner_id
), cancelled AS (
  UPDATE thread.turns t
  SET status = $4, settled_at = clock_timestamp(), parked_at = NULL,
      opened_at = CASE WHEN t.producer IS NOT NULL THEN COALESCE(t.opened_at, now())
                       ELSE t.opened_at END,
      updated_at = now()
  FROM target
  WHERE t.thread_id = $2 AND t.settled_at IS NULL AND NOT target.settled
    AND (t.id = target.id OR EXISTS (
      SELECT 1 FROM run
      WHERE (t.id = run.turn_id AND t.run_fence = run.fencing_token)
         OR (t.input_claim_run_id = run.id AND t.input_claim_fence = run.fencing_token)))
  RETURNING t.id
), closed AS (
  UPDATE fleet.approval_gates g
  SET status = $6, resolved_at = clock_timestamp(), resolved_by = $7, detail = $8,
      updated_at = now()
  FROM cancelled c
  WHERE g.fleet_id = $2 AND g.turn_id = c.id AND g.resolved_at IS NULL
  RETURNING g.id
)
SELECT t.sequence, t.status, t.settled,
       EXISTS (SELECT 1 FROM run) AS running,
       COALESCE((SELECT array_agg(m.runner_id ORDER BY m.id) FROM marked m), '{}') AS runners,
       COALESCE((SELECT array_agg(m.id ORDER BY m.id) FROM marked m), '{}') AS runs,
       (SELECT count(*) FROM cancelled) AS cancelled,
       (SELECT count(*) FROM closed) AS closed
FROM target t

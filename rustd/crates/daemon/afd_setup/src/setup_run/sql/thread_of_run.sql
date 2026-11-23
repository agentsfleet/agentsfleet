-- The owner of the thread a run's context save names: the run's fleet or its draft, when the
-- run is one this runner was assigned. The save itself proves the run still holds the thread.
--
-- $1 run · $2 runner
SELECT r.fleet_id, r.draft_id
  FROM thread.runs r
 WHERE r.id = $1 AND r.runner_id = $2

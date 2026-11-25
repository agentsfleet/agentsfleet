-- The thread a run's context read names, and whether the run still holds it: the run's fleet,
-- its fence and its status. No row for another runner's run.
--
-- $1 run · $2 runner
SELECT l.fleet_id, l.fencing_token, l.status
  FROM thread.runs l
 WHERE l.id = $1 AND l.runner_id = $2

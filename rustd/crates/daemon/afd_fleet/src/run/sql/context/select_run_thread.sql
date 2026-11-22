-- The thread a run's context read names, and whether the run still holds it: the run's owner
-- (its fleet or its draft, exactly one by `ck_runs_one_owner`), which of the two it is, its
-- fence and its status. No row for another runner's run.
--
-- $1 run · $2 runner
SELECT COALESCE(l.fleet_id, l.draft_id) AS owner_id, l.fleet_id IS NOT NULL AS fleet_owned,
       l.fencing_token, l.status
  FROM thread.runs l
 WHERE l.id = $1 AND l.runner_id = $2

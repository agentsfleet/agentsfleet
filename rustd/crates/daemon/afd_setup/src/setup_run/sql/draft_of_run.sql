-- The draft a setup run's context save names: the run's draft, when the run is a draft run
-- this runner was assigned. The save itself proves the run still holds it.
--
-- $1 run · $2 runner
SELECT r.draft_id
  FROM thread.runs r
 WHERE r.id = $1 AND r.runner_id = $2 AND r.draft_id IS NOT NULL

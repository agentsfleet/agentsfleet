-- A deleted draft's active runs, deleted with it: `thread.runs` holds no foreign key to
-- drafts. The runner's next renewal then finds no run and stops.
--
-- $1 draft · $2 active run status
DELETE FROM thread.runs
 WHERE draft_id = $1 AND status = $2
RETURNING runner_id, id

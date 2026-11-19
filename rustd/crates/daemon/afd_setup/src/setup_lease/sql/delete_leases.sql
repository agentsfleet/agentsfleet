-- A deleted draft's active leases, deleted with it: `runner.leases` holds no foreign key to
-- drafts. The runner's next renewal then finds no lease and stops.
--
-- $1 draft · $2 active lease status
DELETE FROM runner.leases
 WHERE draft_id = $1 AND status = $2
RETURNING runner_id, id

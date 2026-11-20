-- The reassignment both kinds share. An assignment that takes work its thread held before
-- expires that work's newest earlier run, still active only when its holder lapsed without
-- reporting, and tallies the expiry against that holder. The caller holds the thread its
-- assignment won, so the lock order is thread, then run, as in the fleet's assignment
-- (`assign_turn_head.sql`).
--
-- $1 the owning fleet, NULL for a draft · $2 the owning draft, NULL for a fleet ·
-- $3 the event or draft message assigned · $4 active status · $5 expired status
WITH prior_run AS MATERIALIZED (
  SELECT l.id
  FROM thread.runs l
  WHERE l.turn_id = $3 AND COALESCE(l.fleet_id, l.draft_id) = COALESCE($1::uuid, $2::uuid)
  ORDER BY l.fencing_token DESC
  LIMIT 1
  FOR UPDATE OF l
), bumped AS (
  UPDATE thread.runs l SET status = $5, updated_at = now()
  FROM prior_run p WHERE l.id = p.id AND l.status = $4
  RETURNING l.runner_id
), tally AS (
  INSERT INTO runner.lifetime_counters AS c
    (runner_id, acquired_count, succeeded_count, failed_count, expired_count,
     created_at, updated_at)
  SELECT runner_id, 0, 0, 0, 1, now(), now() FROM bumped
  ON CONFLICT (runner_id) DO UPDATE
    SET expired_count = c.expired_count + 1, updated_at = now()
)
-- The runs this work was issued before this assignment. A report settles its work, so each
-- one is a run that lapsed without reporting.
SELECT p.id AS predecessor_run_id,
       (SELECT count(*) FROM thread.runs x
         WHERE x.turn_id = $3
           AND COALESCE(x.fleet_id, x.draft_id) = COALESCE($1::uuid, $2::uuid))
           AS earlier_runs
FROM (SELECT 1) AS one
LEFT JOIN prior_run p ON true

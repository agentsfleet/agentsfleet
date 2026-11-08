-- Settles the messages a run folded and read, in the report's transaction,
-- and answers their ids: open_folded_events.sql and close_folded_events.sql
-- then give each its own event, closed with the run's answer, in one
-- statement each however many there are. A message the run claimed but no
-- completed model call read stays unsettled, and the fleet's next run takes
-- it in order: a claim by a lease that has ended binds nothing.
--
-- $1 fleet · $2 lease · $3 fence · $4 read through sequence
UPDATE fleet.admissions a
SET settled_at = clock_timestamp(), parked_at = NULL, updated_at = now()
WHERE a.fleet_id = $1
  AND a.input_claim_lease_id = $2 AND a.input_claim_fence = $3
  AND a.settled_at IS NULL AND a.sequence <= $4
RETURNING a.id AS id

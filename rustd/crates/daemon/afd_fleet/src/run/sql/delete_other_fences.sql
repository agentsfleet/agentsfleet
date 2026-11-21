-- Every item another run of the same turn kept, whichever turn each names: those runs
-- re-ran the turn, so only the current run's items describe the run whose answer stands.
DELETE FROM thread.items t
USING thread.runs r
WHERE t.run_id = r.id
  AND r.fleet_id = $1 AND r.turn_id = $2 AND r.id <> $3

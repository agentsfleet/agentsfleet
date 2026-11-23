-- NO KEY UPDATE serializes the fleet's memory writers and grant changes and still lets a key
-- share through, so an item insert of this fleet's run passes while this capture waits on the
-- run. FOR UPDATE refused that key share and closed a deadlock with the tool-call post.
SELECT id FROM fleet.fleets WHERE id = $1 FOR NO KEY UPDATE

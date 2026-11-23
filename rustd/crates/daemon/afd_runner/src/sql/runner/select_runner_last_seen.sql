-- The beat a connection's end counts its runner's offline instant from. The never-seen
-- sentinel answers no row: a runner that never beat has not stopped.
SELECT last_seen_at FROM runner.runners WHERE id = $1 AND last_seen_at <> $2::timestamptz

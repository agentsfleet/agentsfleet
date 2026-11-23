-- One history row, written in the transaction of the change it records.
-- $1 id · $2 runner · $3 event type · $4 metadata
INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
VALUES ($1, $2, $3, $4, NULL, now())

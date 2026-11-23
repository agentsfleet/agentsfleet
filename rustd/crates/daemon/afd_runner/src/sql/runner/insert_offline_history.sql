-- One `runner_offline` row per silent episode: a runner's `last_seen_at` is unique in
-- its history (`uq_history_runner_id_last_seen_at`), so a second visit writes nothing.
-- $1 id · $2 runner · $3 event type · $4 metadata · $5 the episode's last beat
INSERT INTO runner.history (id, runner_id, event_type, metadata, last_seen_at, created_at)
VALUES ($1, $2, $3, $4, $5, now())
ON CONFLICT (runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL DO NOTHING

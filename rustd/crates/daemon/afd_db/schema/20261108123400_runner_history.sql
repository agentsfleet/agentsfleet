-- Append-only, and kept until a retention policy is chosen. `last_seen_at` is set only on offline
-- events, to the heartbeat observed, so two sweeps over one gap write one row.
CREATE TABLE runner.history (
    id            UUID PRIMARY KEY,
    runner_id     UUID   NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    event_type    TEXT   NOT NULL,
    metadata      JSONB  NOT NULL,
    last_seen_at  TIMESTAMPTZ(3),
    created_at    TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX uq_history_runner_id_last_seen_at
    ON runner.history (runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL;

CREATE INDEX idx_history_runner_id_created_at_id
    ON runner.history (runner_id, created_at DESC, id DESC);

CREATE INDEX idx_history_runner_id_event_type_created_at_id
    ON runner.history (runner_id, event_type, created_at DESC, id DESC);

CREATE FUNCTION runner.events_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'events is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_history_refuse_update
    BEFORE UPDATE ON runner.history
    FOR EACH ROW EXECUTE FUNCTION runner.events_refuse_update();

GRANT SELECT, INSERT ON runner.history TO api_runtime;

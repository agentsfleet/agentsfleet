-- Append-only; DELETE serves the retention sweep. `last_seen_at` is set only on offline events, to
-- the heartbeat observed, so two sweeps over one gap write one row.
CREATE TABLE runner.events (
    id            UUID PRIMARY KEY,
    runner_id     UUID   NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    event_type    TEXT   NOT NULL,
    metadata      JSONB  NOT NULL,
    last_seen_at  TIMESTAMPTZ(3),
    created_at    TIMESTAMPTZ(3) NOT NULL
);

CREATE UNIQUE INDEX uq_events_runner_id_last_seen_at
    ON runner.events (runner_id, last_seen_at) WHERE last_seen_at IS NOT NULL;

CREATE INDEX idx_events_runner_id_created_at_id
    ON runner.events (runner_id, created_at DESC, id DESC);

CREATE INDEX idx_events_runner_id_event_type_created_at_id
    ON runner.events (runner_id, event_type, created_at DESC, id DESC);

CREATE INDEX idx_events_event_type_created_at ON runner.events (event_type, created_at);

CREATE FUNCTION runner.events_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'events is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_events_refuse_update
    BEFORE UPDATE ON runner.events
    FOR EACH ROW EXECUTE FUNCTION runner.events_refuse_update();

GRANT SELECT, INSERT, DELETE ON runner.events TO api_runtime;

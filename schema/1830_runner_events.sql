-- Append-only runner history: enrolment, offline, self-test, policy changes. Read
-- by the runner detail page and the offline sweep. Append-only (rule 9); DELETE
-- serves the retention sweep.
--
-- `dedup_key` makes a repeated event a no-op: the offline sweep stamps the
-- heartbeat it observed, so two sweeps over one gap write one row.

CREATE TABLE runner.events (
    id          UUID PRIMARY KEY,
    CONSTRAINT ck_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    runner_id   UUID   NOT NULL REFERENCES runner.runners(id) ON DELETE CASCADE,
    event_type  TEXT   NOT NULL,
    metadata    JSONB  NOT NULL,
    dedup_key   BIGINT,
    created_at  BIGINT NOT NULL
);

CREATE UNIQUE INDEX uq_events_runner_id_dedup_key
    ON runner.events (runner_id, dedup_key) WHERE dedup_key IS NOT NULL;
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

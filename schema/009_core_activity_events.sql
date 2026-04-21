-- M1_001 §6.3: Activity event log — append-only.
-- One row per action taken by a Zombie. Cursor-based pagination via (zombie_id, created_at).
-- No updated_at: append-only by design. UPDATE and DELETE blocked by trigger.

CREATE TABLE IF NOT EXISTS core.activity_events (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_activity_events_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    zombie_id       UUID NOT NULL REFERENCES core.zombies(id),
    workspace_id    UUID NOT NULL REFERENCES core.workspaces(workspace_id),
    event_type      TEXT NOT NULL,
    detail          TEXT NOT NULL DEFAULT '',
    created_at      BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_activity_events_zombie_created
    ON core.activity_events (zombie_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_activity_events_workspace_created
    ON core.activity_events (workspace_id, created_at DESC);

CREATE OR REPLACE FUNCTION core.activity_events_append_only() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'activity_events is append-only — UPDATE and DELETE are not permitted';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_activity_events_append_only
    BEFORE UPDATE OR DELETE ON core.activity_events
    FOR EACH ROW EXECUTE FUNCTION core.activity_events_append_only();

-- Worker writes during Zombie execution. API reads for zombiectl logs.
GRANT INSERT ON core.activity_events TO worker_runtime;
GRANT SELECT ON core.activity_events TO worker_runtime;
GRANT SELECT ON core.activity_events TO api_runtime;

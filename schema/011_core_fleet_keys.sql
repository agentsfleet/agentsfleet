-- Workspace-scoped fleet keys for Path B callers
-- (LangGraph, CrewAI, Composio). Each external fleet gets a companion
-- fleet record so the full integration grant system applies identically
-- to internal and external callers.
-- key_hash: SHA-256 hex of the raw agt_a key. Raw key shown once at creation.

CREATE TABLE IF NOT EXISTS core.fleet_keys (
    uid             UUID    PRIMARY KEY,
    fleet_key_id        TEXT    NOT NULL UNIQUE,
    workspace_id    UUID    NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    fleet_id       UUID    NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    description     TEXT    NOT NULL,
    key_hash        TEXT    NOT NULL,
    created_at      BIGINT  NOT NULL,
    last_used_at    BIGINT  NULL,
    CONSTRAINT ck_fleet_keys_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    CONSTRAINT ck_fleet_keys_fleet_key_id_uuidv7
        CHECK (fleet_key_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'),
    CONSTRAINT ck_fleet_keys_uid_matches_fleet_key_id CHECK (uid::text = fleet_key_id),
    CONSTRAINT uq_fleet_keys_key_hash UNIQUE (key_hash),
    CONSTRAINT uq_fleet_keys_fleet_id UNIQUE (fleet_id)
);

CREATE INDEX IF NOT EXISTS idx_fleet_keys_workspace_id
    ON core.fleet_keys (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.fleet_keys TO api_runtime;

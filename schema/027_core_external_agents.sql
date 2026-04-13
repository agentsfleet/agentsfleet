-- M9_001: External agent keys for Path B access (LangGraph, CrewAI, Composio).
-- Each external agent gets a companion zombie record so the full integration
-- grant system applies identically to internal and external callers.
-- key_hash: SHA-256 hex of the raw zmb_ key. Raw key shown once at creation.

CREATE TABLE IF NOT EXISTS core.external_agents (
    agent_id        TEXT    NOT NULL PRIMARY KEY,
    workspace_id    UUID    NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    zombie_id       UUID    NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    description     TEXT    NOT NULL DEFAULT '',
    key_hash        TEXT    NOT NULL,
    created_at      BIGINT  NOT NULL,
    last_used_at    BIGINT  NULL,
    CONSTRAINT uq_external_agents_key_hash UNIQUE (key_hash),
    CONSTRAINT uq_external_agents_zombie UNIQUE (zombie_id)
);

CREATE INDEX IF NOT EXISTS idx_external_agents_workspace_id
    ON core.external_agents (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.external_agents TO api_runtime;

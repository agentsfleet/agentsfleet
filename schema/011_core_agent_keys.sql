-- Renamed from core.external_agents → core.agent_keys.
-- Purpose unchanged: workspace-scoped agent keys for Path B callers
-- (LangGraph, CrewAI, Composio). Each external agent gets a companion
-- zombie record so the full integration grant system applies identically
-- to internal and external callers.
-- key_hash: SHA-256 hex of the raw zmb_ key. Raw key shown once at creation.
-- Pre-v2.0 teardown: full file replace of the prior 027_core_external_agents.sql.

CREATE TABLE IF NOT EXISTS core.agent_keys (
    uid             UUID    PRIMARY KEY,
    agent_id        TEXT    NOT NULL UNIQUE,
    workspace_id    UUID    NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    zombie_id       UUID    NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    description     TEXT    NOT NULL,
    key_hash        TEXT    NOT NULL,
    created_at      BIGINT  NOT NULL,
    last_used_at    BIGINT  NULL,
    CONSTRAINT ck_agent_keys_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    CONSTRAINT ck_agent_keys_agent_id_uuidv7
        CHECK (agent_id ~ '^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'),
    CONSTRAINT ck_agent_keys_uid_matches_agent_id CHECK (uid::text = agent_id),
    CONSTRAINT uq_agent_keys_key_hash UNIQUE (key_hash),
    CONSTRAINT uq_agent_keys_zombie UNIQUE (zombie_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_keys_workspace_id
    ON core.agent_keys (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.agent_keys TO api_runtime;

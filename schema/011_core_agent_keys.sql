-- Renamed from core.external_agents → core.agent_keys.
-- Purpose unchanged: workspace-scoped agent keys for Path B callers
-- (LangGraph, CrewAI, Composio). Each external agent gets a companion
-- zombie record so the full integration grant system applies identically
-- to internal and external callers.
-- key_hash: SHA-256 hex of the raw zmb_ key. Raw key shown once at creation.
-- Pre-v2.0 teardown: full file replace of the prior 027_core_external_agents.sql.

CREATE TABLE IF NOT EXISTS core.agent_keys (
    agent_id        TEXT    NOT NULL PRIMARY KEY,
    workspace_id    UUID    NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    zombie_id       UUID    NOT NULL REFERENCES core.zombies(id) ON DELETE CASCADE,
    name            TEXT    NOT NULL,
    description     TEXT    NOT NULL DEFAULT '',
    key_hash        TEXT    NOT NULL,
    created_at      BIGINT  NOT NULL,
    last_used_at    BIGINT  NULL,
    CONSTRAINT uq_agent_keys_key_hash UNIQUE (key_hash),
    CONSTRAINT uq_agent_keys_zombie UNIQUE (zombie_id)
);

CREATE INDEX IF NOT EXISTS idx_agent_keys_workspace_id
    ON core.agent_keys (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON core.agent_keys TO api_runtime;

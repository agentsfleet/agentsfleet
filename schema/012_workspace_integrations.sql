-- 013_workspace_integrations.sql
-- M8_001: Workspace-level provider integration table.
--
-- Purpose: routing metadata only.
--   - Maps external provider team/org ID back to a usezombie workspace_id
--     so incoming Slack events can be routed to the right workspace.
--   - Records which OAuth scopes were granted at install time (audit/display).
--
-- Credential storage: NOT here. The bot token is stored in vault.secrets
-- (workspace_id, key_name="slack") by the OAuth callback handler.
-- The execute pipeline reads from vault directly — this table is never
-- consulted for credential injection.
--
-- CLI path: `zombiectl credential add slack` stores a token in the same
-- vault.secrets slot and creates a row here with source='cli'.

CREATE TABLE IF NOT EXISTS core.workspace_integrations (
    integration_id  UUID    PRIMARY KEY,
    workspace_id    UUID    NOT NULL REFERENCES core.workspaces(workspace_id),
    provider        TEXT    NOT NULL,
    external_id     TEXT    NOT NULL,
    scopes_granted  TEXT    NOT NULL DEFAULT '',
    source          TEXT    NOT NULL DEFAULT 'oauth',  -- 'oauth' | 'cli'
    status          TEXT    NOT NULL DEFAULT 'active', -- 'active' | 'paused' | 'revoked'
    installed_at    BIGINT  NOT NULL,
    updated_at      BIGINT  NOT NULL,
    UNIQUE(provider, external_id)
);

CREATE INDEX IF NOT EXISTS idx_workspace_integrations_workspace
    ON core.workspace_integrations(workspace_id);

CREATE INDEX IF NOT EXISTS idx_workspace_integrations_lookup
    ON core.workspace_integrations(provider, external_id);

-- api_runtime: upsertIntegration (OAuth callback) + lookupWorkspace (Slack event routing)
GRANT SELECT, INSERT, UPDATE ON core.workspace_integrations TO api_runtime;
-- worker_runtime: lookupWorkspace for event dispatch
GRANT SELECT ON core.workspace_integrations TO worker_runtime;

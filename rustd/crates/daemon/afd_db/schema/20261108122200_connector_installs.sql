-- An outside account (a Slack team, a GitHub installation) bound to a workspace. Credentials
-- stay in the vault; this row is only the binding.
CREATE TABLE connector.installs (
    id                   UUID PRIMARY KEY,
    provider             TEXT   NOT NULL,
    external_account_id  TEXT   NOT NULL,
    workspace_id         UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    scopes               TEXT[] NOT NULL,
    created_at           TIMESTAMPTZ(3) NOT NULL,
    updated_at           TIMESTAMPTZ(3) NOT NULL,
    -- One workspace per outside account at a time; webhooks resolve the workspace by this pair.
    CONSTRAINT uq_installs_provider_external_account_id UNIQUE (provider, external_account_id)
);

CREATE INDEX idx_installs_workspace_id ON connector.installs (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON connector.installs TO api_runtime;

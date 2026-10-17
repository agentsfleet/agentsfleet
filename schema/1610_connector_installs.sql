-- An outside account (a Slack team, a GitHub installation) bound to a workspace.
-- Credentials stay in the vault; this row is only the binding. `scopes` is read by
-- the setup review, which blocks a connection whose granted scopes fall short.

CREATE TABLE connector.installs (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_installs_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    provider             TEXT   NOT NULL,
    external_account_id  TEXT   NOT NULL,
    workspace_id         UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    scopes               TEXT[] NOT NULL,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    -- An outside account installs into one workspace at a time; webhooks resolve
    -- the workspace by this pair.
    CONSTRAINT uq_installs_provider_external_account_id UNIQUE (provider, external_account_id)
);

CREATE INDEX idx_installs_workspace_id ON connector.installs (workspace_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON connector.installs TO api_runtime;

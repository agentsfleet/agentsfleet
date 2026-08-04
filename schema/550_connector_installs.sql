-- Provider-keyed inbound-routing index for OAuth connector installs. Maps an
-- external account (provider, external_account_id) back to the agentsfleet
-- workspace that installed the connector, so a signature-authenticated inbound
-- event — which arrives addressed only by the provider's own account identifier,
-- such as Slack's team_id — can resolve its workspace.
--
-- Connector credentials and install metadata live in workspace vault handles,
-- NEVER in this table (RULE VLT) — there is deliberately no token column. Slack
-- events resolve team_id through this index; verified GitHub App events resolve
-- installation_id through the same index before repository, event, grant and
-- fleet routing filters apply.
--
-- Provider values are app-enforced named constants in `src/lib/common/constants.zig`,
-- not a SQL CHECK (RULE STS).

CREATE TABLE IF NOT EXISTS core.connector_installs (
    id                  UUID   PRIMARY KEY,
    CONSTRAINT ck_connector_installs_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    provider            TEXT   NOT NULL,
    external_account_id TEXT   NOT NULL,
    workspace_id        UUID   NOT NULL REFERENCES core.workspaces(id) ON DELETE CASCADE,
    installed_by        TEXT   NOT NULL,
    scopes              TEXT[] NOT NULL,
    created_at          BIGINT NOT NULL,
    updated_at          BIGINT NOT NULL,
    -- The inbound resolve path's whole access path, and the reason a second
    -- install of the same external account cannot shadow the first. Holds a
    -- different value from `id` rather than duplicating it, and it is what the
    -- OAuth callback upsert arbitrates.
    CONSTRAINT uq_connector_installs_provider_external_account_id
        UNIQUE (provider, external_account_id)
);

-- Reader: the dashboard connector roster, which lists a workspace's installs and
-- their connected state; also the index the erasure cascade walks. The unique
-- constraint above leads with `provider`, so it cannot serve a workspace filter.
CREATE INDEX IF NOT EXISTS idx_connector_installs_workspace_id
    ON core.connector_installs (workspace_id);

-- api_runtime: the OAuth callback upsert (INSERT/UPDATE) and the inbound
-- external-account → workspace resolve (SELECT) in the events ingress.
GRANT SELECT, INSERT, UPDATE ON core.connector_installs TO api_runtime;

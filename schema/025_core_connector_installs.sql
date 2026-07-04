-- Generic, provider-keyed inbound-routing index for OAuth connector installs.
-- Maps an external account (provider, external_account_id) back to the
-- agentsfleet workspace that installed the connector, so a
-- signature-authenticated inbound event — which arrives addressed only by the
-- provider's account id (e.g. Slack's team_id) — can resolve its workspace.
--
-- The bot token and all install metadata live in the (workspace_id,
-- 'fleet:slack') vault handle, NEVER in this table (RULE VLT) — there is
-- deliberately no token column. This row is the ONE addition the GitHub
-- connector does not need: GitHub's inbound webhooks are per-fleet-URL
-- addressed, whereas Slack events are addressed by team_id only.
--
-- Value constraints (provider is one of the known connector providers) are
-- enforced in application code via named constants in
-- src/lib/common/constants.zig — RULE STS forbids static-string CHECKs.
CREATE TABLE IF NOT EXISTS core.connector_installs (
    uid                 UUID    PRIMARY KEY,
    CONSTRAINT ck_connector_installs_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    provider            TEXT    NOT NULL,
    external_account_id TEXT    NOT NULL,
    workspace_id        UUID    NOT NULL REFERENCES core.workspaces(workspace_id) ON DELETE CASCADE,
    installed_by        TEXT    NOT NULL,
    scopes              TEXT[]  NOT NULL,
    created_at          BIGINT  NOT NULL,
    updated_at          BIGINT  NOT NULL,
    CONSTRAINT uq_connector_installs_provider_account
        UNIQUE (provider, external_account_id)
);

-- Dashboard roster: list a workspace's connector installs / connected state.
CREATE INDEX IF NOT EXISTS idx_connector_installs_workspace_id
    ON core.connector_installs (workspace_id);

-- api_runtime: OAuth callback upsert (INSERT/UPDATE) + inbound
-- external-account -> workspace resolve (SELECT) in the events ingress.
GRANT SELECT, INSERT, UPDATE ON core.connector_installs TO api_runtime;

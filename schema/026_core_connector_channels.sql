-- Generic, provider-keyed channel -> fleet binding. Maps an external channel
-- (provider, external_account_id, external_channel_id) to the durable resident
-- fleet that owns that channel's memory namespace. For Slack this is
-- (slack, team_id, channel_id) -> channel_fleet_id; the binding is created on
-- the first @mention by calling the shared fleet-insert path, and is the
-- server-derived source of the memory scope (no client ever supplies a scope).
--
-- Insert-once by design (append-only, hence no updated_at): the binding is
-- created under the UNIQUE constraint with ON CONFLICT DO NOTHING so concurrent
-- first-mentions converge on exactly one resident fleet, and is never updated
-- (api_runtime is granted no UPDATE). If the resident fleet is deleted, the row
-- cascades away and the next mention re-materializes it.
--
-- Value constraints (provider is a known connector provider; kind is a known
-- binding kind) are enforced in application code via named constants — RULE STS
-- forbids static-string CHECKs.
CREATE TABLE IF NOT EXISTS core.connector_channels (
    uid                 UUID    PRIMARY KEY,
    CONSTRAINT ck_connector_channels_uid_uuidv7 CHECK (substring(uid::text from 15 for 1) = '7'),
    provider            TEXT    NOT NULL,
    external_account_id TEXT    NOT NULL,
    external_channel_id TEXT    NOT NULL,
    fleet_id            UUID    NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    kind                TEXT    NOT NULL,
    created_at          BIGINT  NOT NULL,
    CONSTRAINT uq_connector_channels_provider_account_channel
        UNIQUE (provider, external_account_id, external_channel_id)
);

-- Reverse lookup (fleet_id -> channel) to post the answer back in-thread, and
-- index support for the fleet foreign-key cascade on fleet deletion.
CREATE INDEX IF NOT EXISTS idx_connector_channels_fleet_id
    ON core.connector_channels (fleet_id);

-- api_runtime: resolve (SELECT) + materialization insert (INSERT) in the events
-- ingress; reverse lookup (SELECT) on the post-back. No UPDATE (append-only).
GRANT SELECT, INSERT ON core.connector_channels TO api_runtime;

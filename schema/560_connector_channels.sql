-- Provider-keyed channel → fleet binding. Maps an external channel
-- (provider, external_account_id, external_channel_id) to the durable resident
-- fleet that owns that channel's memory namespace. For Slack that is
-- (slack, team_id, channel_id) → the channel's fleet. The binding is created on
-- the first mention by calling the shared fleet-insert path, and is the
-- server-derived source of the memory scope — no client ever supplies a scope.
--
-- Insert-once by design: the binding is created under the unique constraint with
-- ON CONFLICT DO NOTHING, so concurrent first-mentions converge on exactly one
-- resident fleet, and it is never updated (api_runtime is granted no UPDATE). If
-- the resident fleet is deleted the row cascades away, and the next mention
-- re-materialises it.
--
-- Provider and binding-kind vocabularies are app-enforced named constants
-- (RULE STS).

CREATE TABLE IF NOT EXISTS core.connector_channels (
    id                  UUID   PRIMARY KEY,
    CONSTRAINT ck_connector_channels_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    provider            TEXT   NOT NULL,
    external_account_id TEXT   NOT NULL,
    external_channel_id TEXT   NOT NULL,
    fleet_id            UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    kind                TEXT   NOT NULL,
    created_at          BIGINT NOT NULL,
    -- The resolve path's whole access path, and what makes concurrent
    -- first-mentions converge rather than double-materialise. Holds a different
    -- value from `id` rather than duplicating it.
    CONSTRAINT uq_connector_channels_provider_account_channel
        UNIQUE (provider, external_account_id, external_channel_id)
);

-- No `updated_at`: the row is append-only by grant. Nothing holds UPDATE on this
-- table, so a row-change time could never be written.

-- Reader: the reverse lookup (fleet → channel) that posts an answer back
-- in-thread, and the index the fleet cascade walks on fleet deletion. The unique
-- constraint above leads with `provider`, so it cannot serve either.
CREATE INDEX IF NOT EXISTS idx_connector_channels_fleet_id
    ON core.connector_channels (fleet_id);

-- api_runtime: resolve (SELECT) and the materialisation insert (INSERT) in the
-- events ingress, plus the reverse lookup (SELECT) on the post-back.
GRANT SELECT, INSERT ON core.connector_channels TO api_runtime;

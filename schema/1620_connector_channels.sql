-- A Slack channel bound to its resident fleet. A table, not an installs column,
-- because one account has many channels. When a team moves workspace the binding
-- is rebound in place, which is the only UPDATE this table takes.

CREATE TABLE connector.channels (
    id                   UUID PRIMARY KEY,
    CONSTRAINT ck_channels_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    provider             TEXT   NOT NULL,
    external_account_id  TEXT   NOT NULL,
    external_channel_id  TEXT   NOT NULL,
    fleet_id             UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    binding_type         TEXT   NOT NULL,
    created_at           BIGINT NOT NULL,
    updated_at           BIGINT NOT NULL,
    CONSTRAINT uq_channels_provider_external_account_id_external_channel_id
        UNIQUE (provider, external_account_id, external_channel_id)
);

CREATE INDEX idx_channels_fleet_id ON connector.channels (fleet_id);

-- A rebind moves the fleet and binding type; identity columns stay fixed.
GRANT SELECT, INSERT ON connector.channels TO api_runtime;
GRANT UPDATE (fleet_id, binding_type, updated_at) ON connector.channels TO api_runtime;

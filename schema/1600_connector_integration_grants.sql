-- Whether a fleet may use an outside service. A table, not a fleet column, because
-- a fleet holds one grant per service, each with its own approval history. A deny
-- is stored as revoked. `approved_at` and `revoked_at` are instants the grant list
-- returns (rule 4c).

CREATE TABLE connector.integration_grants (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_integration_grants_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id          UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    service           TEXT   NOT NULL,
    status            TEXT   NOT NULL,
    requested_reason  TEXT   NOT NULL,
    approved_at       TIMESTAMPTZ(3),
    revoked_at        TIMESTAMPTZ(3),
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_integration_grants_fleet_id_service UNIQUE (fleet_id, service)
);

GRANT SELECT, INSERT, UPDATE ON connector.integration_grants TO api_runtime;

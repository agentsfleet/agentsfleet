-- One grant per (fleet, service): the fleet asks for a workspace secret or a connected
-- integration by name, and a person approves it once. A deny is stored as revoked, and a later
-- request moves the row back to pending. `kind` (`secret` or `integration`, spelled by
-- `afd_api_wire::grant::kind`) says which channel the credential reaches a run through; the run's
-- policy build and the token mint read it. `scope` is the approved GitHub scope, `requested_scope`
-- a wider one waiting on a person, both in `af_fleet_runtime::config::Recorded`'s shape; Approve
-- moves the request into `scope`, and until then the approved scope stays in force.
CREATE TABLE connector.integration_grants (
    id                UUID PRIMARY KEY,
    fleet_id          UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    service           TEXT   NOT NULL,
    kind              TEXT   NOT NULL,
    status            TEXT   NOT NULL,
    scope             JSONB,
    requested_scope   JSONB,
    requested_reason  TEXT   NOT NULL,
    approved_at       TIMESTAMPTZ(3),
    revoked_at        TIMESTAMPTZ(3),
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_integration_grants_fleet_id_service UNIQUE (fleet_id, service)
);

GRANT SELECT, INSERT, UPDATE ON connector.integration_grants TO api_runtime;

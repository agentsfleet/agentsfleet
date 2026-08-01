-- Integration grants for fleet-to-service authorization. A fleet must hold an
-- approved grant for a service before agentsfleet will inject credentials for
-- it. Fleet-initiated, human-approved. Status vocabulary is app-enforced
-- (RULE STS).
--
-- Like `core.fleet_keys`, the retired shape carried a generated identity column
-- plus a `grant_id` text twin, a CHECK tying them to the same value, and a
-- full-shape UUID regular expression on the twin. The value is one column now;
-- the public field name
-- `grant_id` is unchanged and aliased at the boundary.
--
-- `requested_at` is gone, not renamed away from its meaning: a grant row is
-- created by the request, so the request instant IS the row's birth and
-- `created_at` says so without inventing a second name for it.
-- `approved_at` and `revoked_at` stay — both are domain instants that may never
-- arrive, and neither means "when this row changed".

CREATE TABLE IF NOT EXISTS core.integration_grants (
    id               UUID   PRIMARY KEY,
    CONSTRAINT ck_integration_grants_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    fleet_id         UUID   NOT NULL REFERENCES core.fleets(id) ON DELETE CASCADE,
    service          TEXT   NOT NULL,
    status           TEXT   NOT NULL,
    requested_reason TEXT   NOT NULL,
    approved_at      BIGINT,
    revoked_at       BIGINT,
    created_at       BIGINT NOT NULL,
    -- A fleet holds at most one grant per service; re-requesting moves the
    -- existing row's status rather than adding a second.
    CONSTRAINT uq_integration_grants_fleet_id_service UNIQUE (fleet_id, service)
);

-- No `updated_at`: every mutation of this row is a status transition, and each
-- transition stamps its own domain instant (`approved_at`, `revoked_at`). A
-- row-change time would carry no information those two do not already carry.

-- No index. The retired `idx_integration_grants_fleet_id` was a strict prefix of
-- the unique constraint above — a btree on (fleet_id) answers nothing a btree on
-- (fleet_id, service) cannot, because the leading column is identical — so it
-- was a second index maintained on every write for queries the constraint
-- already served, including the cascade.

GRANT SELECT, INSERT, UPDATE ON core.integration_grants TO api_runtime;

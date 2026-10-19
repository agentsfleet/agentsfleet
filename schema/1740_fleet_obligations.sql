-- An answer owed to a destination until it is delivered or abandoned. A table, not
-- an events column, because delivery retries outlive the run and are scanned
-- across fleets. One obligation per (fleet, event).
--
-- `sequence` is the global insertion order the delivery scan pages on. `receipt`,
-- `delivered_at` and `abandoned_at` are the NULL markers the delivery indexes read;
-- `abandon_reason` is shown on the event. `destination` is the delivery snapshot
-- (rule 5 exception), frozen when the answer is owed.

CREATE TABLE fleet.obligations (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_obligations_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    sequence        BIGINT GENERATED ALWAYS AS IDENTITY,
    fleet_id        UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id    UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    provider        TEXT   NOT NULL,
    destination     TEXT   NOT NULL,
    event_id        TEXT   NOT NULL,
    answer          TEXT   NOT NULL,
    receipt         TEXT,
    delivered_at    TIMESTAMPTZ(3),
    attempt_count   BIGINT NOT NULL,
    abandoned_at    TIMESTAMPTZ(3),
    abandon_reason  TEXT,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    updated_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_obligations_fleet_id_event_id UNIQUE (fleet_id, event_id),
    CONSTRAINT ck_obligations_abandon_both_or_neither
        CHECK ((abandoned_at IS NULL) = (abandon_reason IS NULL))
);

CREATE INDEX idx_obligations_unreceipted
    ON fleet.obligations (created_at, sequence)
    WHERE receipt IS NULL AND abandoned_at IS NULL;
CREATE INDEX idx_obligations_undelivered
    ON fleet.obligations (fleet_id, created_at, sequence)
    WHERE receipt IS NOT NULL AND delivered_at IS NULL AND abandoned_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON fleet.obligations TO api_runtime;

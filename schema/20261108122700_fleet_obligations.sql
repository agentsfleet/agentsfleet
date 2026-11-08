-- An answer owed to a destination until it is delivered or abandoned. A table, not
-- an events column, because delivery retries outlive the run and are scanned
-- across fleets. One obligation per (fleet, event).
--
-- `sequence` orders a lane, one (provider, workspace) pair. `receipt` is the
-- current holder's token: the report writes a fresh one, every claim writes its
-- own, and every stamp, count, release and abandon matches it, so a lapsed
-- holder's late write changes nothing (M244 §4). `claim_expires_at` is the claim
-- deadline, `next_attempt_at` the retry deadline. `delivered_at` and
-- `abandoned_at` are the NULL markers the open-lane index reads; `abandon_reason`
-- is shown on the event. `destination` is the delivery snapshot (rule 5
-- exception), frozen when the answer is owed.
CREATE TABLE fleet.obligations (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_obligations_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    sequence        BIGINT GENERATED ALWAYS AS IDENTITY,
    fleet_id        UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id    UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    provider        TEXT   NOT NULL,
    destination     TEXT   NOT NULL,
    event_id        UUID   NOT NULL,
    answer          TEXT   NOT NULL,
    receipt         TEXT,
    claim_expires_at   TIMESTAMPTZ(3) NOT NULL,
    next_attempt_at TIMESTAMPTZ(3),
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

-- The claim reads open rows in sequence order and checks each lane for an
-- earlier open row; both walk this index.
CREATE INDEX idx_obligations_open_lane
    ON fleet.obligations (provider, workspace_id, sequence)
    WHERE delivered_at IS NULL AND abandoned_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON fleet.obligations TO api_runtime;

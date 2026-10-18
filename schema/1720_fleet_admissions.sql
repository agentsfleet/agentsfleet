-- The work ledger: every accepted request, keyed by its producer so a retry cannot
-- duplicate work. A table, not an events column, because admission happens before
-- the event exists and must survive a crash in between.
--
-- `sequence` is the global insertion order the delivery cursor pages on.
-- `receipt` and `delivered_at` are the NULL markers the delivery indexes read.
-- `event_created_at` is the producer's instant billing charges against.
-- `reply_provider` and `reply_address` are the delivery snapshot (rule 5
-- exception): where the answer goes, frozen because the source may change.

CREATE TABLE fleet.admissions (
    id                UUID PRIMARY KEY,
    CONSTRAINT ck_admissions_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    sequence          BIGINT GENERATED ALWAYS AS IDENTITY,
    fleet_id          UUID   NOT NULL REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    workspace_id      UUID   NOT NULL REFERENCES identity.workspaces(id) ON DELETE CASCADE,
    producer          TEXT   NOT NULL,
    producer_key      TEXT   NOT NULL,
    payload_digest    TEXT   NOT NULL,
    actor             TEXT   NOT NULL,
    event_type        TEXT   NOT NULL,
    request_json      TEXT   NOT NULL,
    event_created_at  TIMESTAMPTZ(3) NOT NULL,
    reply_provider    TEXT,
    reply_address     TEXT,
    receipt           TEXT,
    delivered_at      TIMESTAMPTZ(3),
    replay_count      BIGINT NOT NULL,
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_admissions_producer_producer_key UNIQUE (producer, producer_key),
    CONSTRAINT ck_admissions_reply_both_or_neither
        CHECK ((reply_provider IS NULL) = (reply_address IS NULL))
);

CREATE INDEX idx_admissions_fleet_id ON fleet.admissions (fleet_id);
-- Not yet receipted: the dispatcher's oldest-first scan.
CREATE INDEX idx_admissions_unreceipted
    ON fleet.admissions (created_at, sequence) WHERE receipt IS NULL;
-- Receipted, not delivered: per-fleet redelivery.
CREATE INDEX idx_admissions_undelivered
    ON fleet.admissions (fleet_id, created_at, sequence)
    WHERE receipt IS NOT NULL AND delivered_at IS NULL;
-- Not delivered, in event order: a fleet's waiting steers, newest first. Keyed
-- on `id`, the admission's `event_id`, so `ORDER BY created_at, id` comes off
-- the index with no sort.
CREATE INDEX idx_admissions_delivery_lookup
    ON fleet.admissions (fleet_id, created_at, id) WHERE delivered_at IS NULL;
CREATE INDEX idx_admissions_reply_lookup
    ON fleet.admissions (fleet_id, created_at, sequence) WHERE reply_provider IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON fleet.admissions TO api_runtime;

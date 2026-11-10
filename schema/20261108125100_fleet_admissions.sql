-- The work ledger: every accepted request, keyed by its producer so a retry cannot
-- duplicate work. A table, not an events column, because admission happens before
-- the event exists and must survive a crash in between.
--
-- `sequence` is the global insertion order the delivery cursor pages on.
-- `receipt` and `delivered_at` are the NULL markers the delivery indexes read.
-- `event_created_at` is the producer's instant billing charges against.
-- `reply_provider` and `reply_address` are the delivery snapshot (rule 5
-- exception): where the answer goes, frozen because the source may change.
-- `replay_claim_expires_at` is a replay pass's claim on an unreceipted row: the
-- pass commits the stamp, appends with no transaction open, then records the
-- receipt and clears it. A claim a dead pass left lapses at this instant.
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
    delivered_at      TIMESTAMPTZ(3),
    created_at        TIMESTAMPTZ(3) NOT NULL,
    updated_at        TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_admissions_producer_producer_key UNIQUE (producer, producer_key),
    CONSTRAINT ck_admissions_reply_both_or_neither
        CHECK ((reply_provider IS NULL) = (reply_address IS NULL)),
    -- The queue state each admission carries beside its accepted event.
    lease_fence BIGINT,
    parked_at TIMESTAMPTZ(3),
    settled_at TIMESTAMPTZ(3),
    task_id UUID,
    task_item_id UUID,
    input_claim_lease_id UUID,
    input_claim_fence BIGINT,
    CONSTRAINT fk_admissions_task_scope FOREIGN KEY (task_id, fleet_id, workspace_id)
        REFERENCES fleet.tasks (id, fleet_id, workspace_id) ON DELETE SET NULL (task_id),
    CONSTRAINT fk_admissions_task_item_scope FOREIGN KEY (task_item_id, fleet_id, workspace_id)
        REFERENCES fleet.task_items (id, fleet_id, workspace_id) ON DELETE SET NULL (task_item_id),
    -- Exact conversation equality and sequence pagination avoid scanning another
    -- conversation's backlog. Settled history stays outside the working index.
    interpreted_at TIMESTAMPTZ(3),
    -- Queue ownership freezes the task generation separately from a lease fence.
    -- Retain this marker if privacy cleanup removes its nullable task reference.
    task_generation BIGINT CHECK (task_generation > 0)
);

CREATE INDEX idx_admissions_fleet_id ON fleet.admissions (fleet_id);

-- Not delivered, in event order: a fleet's waiting steers, newest first. Keyed
-- on `id`, the admission's `event_id`, so `ORDER BY created_at, id` comes off
-- the index with no sort.
CREATE INDEX idx_admissions_delivery_lookup
    ON fleet.admissions (fleet_id, created_at, id) WHERE delivered_at IS NULL;

CREATE INDEX idx_admissions_reply_lookup
    ON fleet.admissions (fleet_id, created_at, sequence) WHERE reply_provider IS NOT NULL;

GRANT SELECT, INSERT, UPDATE ON fleet.admissions TO api_runtime;

GRANT SELECT, INSERT, UPDATE ON fleet.admissions TO api_runtime;

CREATE INDEX idx_admissions_unsettled
    ON fleet.admissions (fleet_id, sequence)
    WHERE settled_at IS NULL;

CREATE INDEX idx_admissions_pending_conversation
    ON fleet.admissions (fleet_id, reply_provider, reply_address, sequence)
    WHERE settled_at IS NULL;

CREATE INDEX idx_admissions_uninterpreted_order ON fleet.admissions (fleet_id, sequence)
    WHERE settled_at IS NULL AND interpreted_at IS NULL;

CREATE INDEX idx_admissions_task_pending ON fleet.admissions (task_id)
    WHERE task_id IS NOT NULL AND settled_at IS NULL;

CREATE INDEX idx_admissions_task_item ON fleet.admissions (task_item_id)
    WHERE task_item_id IS NOT NULL;

CREATE INDEX idx_admissions_task_generation ON fleet.admissions (task_id, task_generation, settled_at, sequence)
    WHERE task_id IS NOT NULL;

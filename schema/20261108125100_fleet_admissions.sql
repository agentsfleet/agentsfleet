-- Every accepted request, keyed by its producer so a retry cannot duplicate work; `id` becomes
-- the event's `event_id`. The reply columns are frozen at admission because the source may change.
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
    interpreted_at TIMESTAMPTZ(3),
    -- Frozen separately from the lease fence, and kept when cleanup clears `task_id`.
    task_generation BIGINT CHECK (task_generation > 0)
);

CREATE INDEX idx_admissions_fleet_id ON fleet.admissions (fleet_id);

CREATE INDEX idx_admissions_delivery_lookup
    ON fleet.admissions (fleet_id, created_at, id) WHERE delivered_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON fleet.admissions TO api_runtime;

CREATE INDEX idx_admissions_unsettled
    ON fleet.admissions (fleet_id, sequence)
    WHERE settled_at IS NULL;

CREATE INDEX idx_admissions_pending_conversation
    ON fleet.admissions (fleet_id, reply_provider, reply_address, sequence)
    WHERE settled_at IS NULL;

CREATE INDEX idx_admissions_task_item ON fleet.admissions (task_item_id)
    WHERE task_item_id IS NOT NULL;

CREATE INDEX idx_admissions_task_generation ON fleet.admissions (task_id, task_generation, settled_at, sequence)
    WHERE task_id IS NOT NULL;

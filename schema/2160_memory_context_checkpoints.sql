-- Private conversation working state shared by setup and execution. Separate
-- from semantic memory and immutable revisions: each has a different owner,
-- deletion boundary and conditional-write rule.
-- The draft or lease/branch owns one row. A runtime fleet foreign key removes
-- private notes on fleet purge even though settled runner leases outlive it.
-- resource_version is the guarded projection of document.generation; the
-- repository writes both from one validated checkpoint in the same statement.

CREATE TABLE memory.context_checkpoints (
    id                  UUID PRIMARY KEY,
    CONSTRAINT ck_context_checkpoints_id_uuidv7
        CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id            UUID REFERENCES authoring.drafts(id) ON DELETE CASCADE,
    lease_id            UUID REFERENCES runner.leases(id) ON DELETE CASCADE,
    fleet_id            UUID REFERENCES fleet.fleets(id) ON DELETE CASCADE,
    branch_id           UUID,
    resource_version    BIGINT NOT NULL,
    document            JSONB NOT NULL,
    created_at          TIMESTAMPTZ(3) NOT NULL,
    updated_at          TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT ck_context_checkpoints_owner CHECK (
        (draft_id IS NOT NULL AND lease_id IS NULL AND fleet_id IS NULL AND branch_id IS NULL)
        OR (draft_id IS NULL AND lease_id IS NOT NULL AND fleet_id IS NOT NULL AND branch_id IS NOT NULL)
    )
);

CREATE UNIQUE INDEX idx_context_checkpoints_draft_id
    ON memory.context_checkpoints (draft_id) WHERE draft_id IS NOT NULL;
CREATE UNIQUE INDEX idx_context_checkpoints_lease_id_branch_id
    ON memory.context_checkpoints (lease_id, branch_id) WHERE lease_id IS NOT NULL;
CREATE INDEX idx_context_checkpoints_fleet_id ON memory.context_checkpoints (fleet_id)
    WHERE fleet_id IS NOT NULL;
GRANT USAGE ON SCHEMA memory TO api_runtime;
GRANT SELECT, INSERT, UPDATE, DELETE ON memory.context_checkpoints TO api_runtime;

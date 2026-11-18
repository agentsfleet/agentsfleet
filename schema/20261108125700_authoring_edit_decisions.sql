-- Accounting for a setup dispatch's extra physical call; it holds no message, instructions or
-- provider prose. Ids are opaque so the record survives private deletion.
CREATE TABLE authoring.edit_decisions (
    dispatch_id UUID PRIMARY KEY,
    attempt_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    usage_sequence BIGINT NOT NULL CHECK (usage_sequence >= 0),
    candidate_revision BIGINT NOT NULL CHECK (candidate_revision >= 0),
    disposition TEXT NOT NULL,
    reserved_nanos BIGINT NOT NULL CHECK (reserved_nanos >= 0),
    -- Measured token categories, recorded whether or not the edit was valid.
    usage JSONB,
    charged_nanos BIGINT CHECK (charged_nanos >= 0),
    -- NULL means usage is unresolved and the reservation is still held.
    accounted_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_edit_decisions_usage UNIQUE (attempt_id, usage_sequence)
);

CREATE INDEX idx_edit_decisions_unresolved ON authoring.edit_decisions (attempt_id)
    WHERE accounted_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON authoring.edit_decisions TO api_runtime;

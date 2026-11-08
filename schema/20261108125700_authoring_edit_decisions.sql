-- A setup dispatch may make a second physical call with different model/prices.
-- This accounting journal retains no message, instructions or provider prose.
-- Opaque work/payer identities survive private deletion; record.rs logs usage
-- with SetupChat attribution. candidate/proposal.rs reads the no-edit veto.
CREATE TABLE authoring.edit_decisions (
    dispatch_id UUID PRIMARY KEY,
    attempt_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    workspace_id UUID NOT NULL,
    usage_sequence BIGINT NOT NULL CHECK (usage_sequence >= 0),
    candidate_revision BIGINT NOT NULL CHECK (candidate_revision >= 0),
    -- Frozen catalogue/price delivery snapshot; no secret or private text.
    snapshot JSONB,
    disposition TEXT NOT NULL,
    -- Non-NULL only when a physical request may have spent the reservation.
    reserved_nanos BIGINT NOT NULL CHECK (reserved_nanos >= 0),
    -- Native measured token categories, independent of semantic validity.
    usage JSONB,
    charged_nanos BIGINT CHECK (charged_nanos >= 0),
    -- NULL means unresolved usage; finished-holds excludes these reservations.
    accounted_at TIMESTAMPTZ(3),
    created_at TIMESTAMPTZ(3) NOT NULL,
    updated_at TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_edit_decisions_usage UNIQUE (attempt_id, usage_sequence)
);

CREATE INDEX idx_edit_decisions_unresolved ON authoring.edit_decisions (attempt_id)
    WHERE accounted_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON authoring.edit_decisions TO api_runtime;

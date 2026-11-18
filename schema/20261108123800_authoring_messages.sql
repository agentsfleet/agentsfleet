-- One chat turn. NULL `tenant_model_id` is the platform default. `operation_id` is not unique:
-- the retry window lives in `authoring.replay_receipts`, and once it passes the same key queues
-- a new turn.
CREATE TABLE authoring.messages (
    id               UUID PRIMARY KEY,
    draft_id         UUID   NOT NULL REFERENCES authoring.drafts(id) ON DELETE CASCADE,
    sequence         BIGINT NOT NULL,
    operation_id     UUID   NOT NULL,
    status           TEXT   NOT NULL,
    tenant_model_id  UUID REFERENCES model.tenant_models(id) ON DELETE SET NULL,
    user_text        TEXT   NOT NULL,
    answer_text      TEXT,
    candidate_id     UUID REFERENCES authoring.candidates(id) ON DELETE SET NULL,
    failure_reason   TEXT,
    finished_at      TIMESTAMPTZ(3),
    created_at       TIMESTAMPTZ(3) NOT NULL,
    updated_at       TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_messages_draft_id_sequence UNIQUE (draft_id, sequence),
    -- A requested save completes in the same transaction as the terminal answer;
    -- `save_error_code` records why a checked save was refused.
    save_requested BOOLEAN NOT NULL DEFAULT false,
    save_error_code TEXT
);

CREATE INDEX idx_messages_status_created_at_id ON authoring.messages (status, created_at, id);

CREATE INDEX idx_messages_candidate_id
    ON authoring.messages (candidate_id) WHERE candidate_id IS NOT NULL;

CREATE INDEX idx_messages_tenant_model_id
    ON authoring.messages (tenant_model_id) WHERE tenant_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.messages TO api_runtime;

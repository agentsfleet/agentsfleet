-- One chat turn: what the person typed and what came back. A table, not a draft
-- column, because a chat has many turns, each run by its own attempt.
--
-- `sequence` is the turn's position in its draft. `status` and `created_at` feed
-- the dispatcher's pending scan. `tenant_model_id` is the model the turn ran on
-- (NULL is the platform default). `candidate_id` is the setup this turn produced.
-- `finished_at` is the instant the turn's answer is shown with (rule 4c).
-- `operation_id` is the client's key for the request that queued the turn; the
-- message API returns it for the turn's whole life. The retry window itself is
-- enforced by `authoring.replay_receipts` (rule 6), whose rows are swept 24 hours
-- after they are filed while the turn lives as long as its draft, so the key the
-- API returns cannot be read back from the receipt. Not unique: once the window
-- passes, the same key queues a new turn.

CREATE TABLE authoring.messages (
    id               UUID PRIMARY KEY,
    CONSTRAINT ck_messages_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
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
    CONSTRAINT uq_messages_draft_id_sequence UNIQUE (draft_id, sequence)
);

CREATE INDEX idx_messages_status_created_at_id ON authoring.messages (status, created_at, id);
CREATE INDEX idx_messages_candidate_id
    ON authoring.messages (candidate_id) WHERE candidate_id IS NOT NULL;
CREATE INDEX idx_messages_tenant_model_id
    ON authoring.messages (tenant_model_id) WHERE tenant_model_id IS NOT NULL;

GRANT SELECT, INSERT, UPDATE, DELETE ON authoring.messages TO api_runtime;

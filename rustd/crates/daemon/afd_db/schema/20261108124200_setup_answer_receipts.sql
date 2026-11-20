-- Replay proof: a retried answer report with the same report id gets the first answer.
-- Append-only; it goes with its draft.
CREATE TABLE setup.answer_receipts (
    id             UUID PRIMARY KEY,
    draft_id       UUID   NOT NULL REFERENCES setup.drafts(id) ON DELETE CASCADE,
    attempt_id     UUID   NOT NULL REFERENCES setup.attempts(id) ON DELETE CASCADE,
    report_id      UUID   NOT NULL,
    answer_digest  TEXT   NOT NULL,
    expires_at     TIMESTAMPTZ(3) NOT NULL,
    created_at     TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_answer_receipts_attempt_id_report_id UNIQUE (attempt_id, report_id)
);

CREATE INDEX idx_answer_receipts_expires_at_id ON setup.answer_receipts (expires_at, id);

CREATE INDEX idx_answer_receipts_draft_id ON setup.answer_receipts (draft_id);

CREATE FUNCTION setup.answer_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'answer_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_answer_receipts_refuse_update
    BEFORE UPDATE ON setup.answer_receipts
    FOR EACH ROW EXECUTE FUNCTION setup.answer_receipts_refuse_update();

GRANT SELECT, INSERT, DELETE ON setup.answer_receipts TO api_runtime;

-- Replay proof for "the runner posted an answer": a retried report with the same
-- report id returns the first answer. Append-only (rule 9); it goes with its draft,
-- and `expires_at` is the deadline the sweep reads.

CREATE TABLE authoring.answer_receipts (
    id             UUID PRIMARY KEY,
    CONSTRAINT ck_answer_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    draft_id       UUID   NOT NULL REFERENCES authoring.drafts(id) ON DELETE CASCADE,
    attempt_id     UUID   NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    report_id      UUID   NOT NULL,
    answer_digest  TEXT   NOT NULL,
    expires_at     TIMESTAMPTZ(3) NOT NULL,
    created_at     TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_answer_receipts_attempt_id_report_id UNIQUE (attempt_id, report_id)
);

CREATE INDEX idx_answer_receipts_expires_at_id ON authoring.answer_receipts (expires_at, id);
CREATE INDEX idx_answer_receipts_draft_id ON authoring.answer_receipts (draft_id);

CREATE FUNCTION authoring.answer_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'answer_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_answer_receipts_refuse_update
    BEFORE UPDATE ON authoring.answer_receipts
    FOR EACH ROW EXECUTE FUNCTION authoring.answer_receipts_refuse_update();

GRANT SELECT, INSERT, DELETE ON authoring.answer_receipts TO api_runtime;

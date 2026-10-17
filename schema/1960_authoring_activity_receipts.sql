-- Replay proof for "the runner posted a setup": a retried activity report with the
-- same sequence and digest returns the first answer. Append-only (rule 9); stays
-- with its attempt. `candidate_id` carries no foreign key, because the candidate is
-- swept with its draft and the receipt must outlive it.

CREATE TABLE authoring.activity_receipts (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_activity_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id      UUID   NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    sequence        BIGINT NOT NULL,
    request_digest  TEXT   NOT NULL,
    candidate_id    UUID,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_activity_receipts_attempt_id_sequence UNIQUE (attempt_id, sequence)
);

CREATE FUNCTION authoring.activity_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'activity_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_activity_receipts_refuse_update
    BEFORE UPDATE ON authoring.activity_receipts
    FOR EACH ROW EXECUTE FUNCTION authoring.activity_receipts_refuse_update();

GRANT SELECT, INSERT ON authoring.activity_receipts TO api_runtime;

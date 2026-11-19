-- Replay proof: a retried activity report with the same sequence and digest gets the first
-- answer. Append-only. `candidate_id` has no foreign key: the candidate is swept with its draft
-- and the receipt must outlive it.
CREATE TABLE authoring.activity_receipts (
    id              UUID PRIMARY KEY,
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

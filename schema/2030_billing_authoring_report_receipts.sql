-- Acknowledges an interim accounting report without settling, so a retried report
-- returns the first acknowledgement. A table, not settlement rows, because an
-- attempt reports many times and settles once. Append-only (rule 9).

CREATE TABLE billing.authoring_report_receipts (
    id                 UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_report_receipts_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id         UUID   NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    report_id          UUID   NOT NULL,
    accounting_digest  TEXT   NOT NULL,
    acknowledgement    JSONB  NOT NULL,
    created_at         TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_authoring_report_receipts_attempt_id_report_id UNIQUE (attempt_id, report_id)
);

CREATE FUNCTION billing.authoring_report_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'authoring_report_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_authoring_report_receipts_refuse_update
    BEFORE UPDATE ON billing.authoring_report_receipts
    FOR EACH ROW EXECUTE FUNCTION billing.authoring_report_receipts_refuse_update();

GRANT SELECT, INSERT ON billing.authoring_report_receipts TO api_runtime;

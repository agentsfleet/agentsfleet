-- Acknowledges an interim accounting report without settling, so a retried report gets the
-- first acknowledgement. Append-only.
CREATE TABLE billing.setup_report_receipts (
    id                 UUID PRIMARY KEY,
    attempt_id         UUID   NOT NULL REFERENCES setup.attempts(id) ON DELETE CASCADE,
    report_id          UUID   NOT NULL,
    accounting_digest  TEXT   NOT NULL,
    acknowledgement    JSONB  NOT NULL,
    created_at         TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_setup_report_receipts_attempt_id_report_id UNIQUE (attempt_id, report_id)
);

CREATE FUNCTION billing.setup_report_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'setup_report_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_setup_report_receipts_refuse_update
    BEFORE UPDATE ON billing.setup_report_receipts
    FOR EACH ROW EXECUTE FUNCTION billing.setup_report_receipts_refuse_update();

GRANT SELECT, INSERT ON billing.setup_report_receipts TO api_runtime;

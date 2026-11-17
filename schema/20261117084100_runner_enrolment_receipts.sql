-- One tenant enrolment's retry key. `operation_id` is the request's
-- Idempotency-Key, and `request_digest` the SHA-256 of the request as parsed,
-- so a replay of the same request and a key reused for another one answer
-- differently. The one-time runner token is never kept here (AUTH-SECRET).
--
-- No foreign keys: the receipt outlives its runner and its tenant, so a key
-- retried after the runner was deleted still enrols nothing (SQL-RETRYKEY).
-- Every column is an opaque identifier or a digest, so tenant erasure leaves
-- it in place, as it leaves the other receipts.
CREATE TABLE runner.enrolment_receipts (
    id              UUID PRIMARY KEY,
    CONSTRAINT ck_enrolment_receipts_id_uuidv7 CHECK (uuid_extract_version(id) IS NOT DISTINCT FROM 7),
    tenant_id       UUID   NOT NULL,
    operation_id    UUID   NOT NULL,
    request_digest  TEXT   NOT NULL,
    runner_id       UUID   NOT NULL,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_enrolment_receipts_tenant_id_operation_id UNIQUE (tenant_id, operation_id)
);

CREATE FUNCTION runner.enrolment_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'enrolment_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_enrolment_receipts_refuse_update
    BEFORE UPDATE ON runner.enrolment_receipts
    FOR EACH ROW EXECUTE FUNCTION runner.enrolment_receipts_refuse_update();

GRANT SELECT, INSERT ON runner.enrolment_receipts TO api_runtime;

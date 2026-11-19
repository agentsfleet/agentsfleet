-- A tenant registration's retry key. `request_digest` is the SHA-256 of the parsed request, so a
-- replay and a reused key answer differently. No foreign keys: the receipt outlives its runner and
-- tenant, so a retry after deletion registers nothing. The runner token is never kept here.
CREATE TABLE runner.registration_receipts (
    id              UUID PRIMARY KEY,
    tenant_id       UUID   NOT NULL,
    operation_id    UUID   NOT NULL,
    request_digest  TEXT   NOT NULL,
    runner_id       UUID   NOT NULL,
    created_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_registration_receipts_tenant_id_operation_id UNIQUE (tenant_id, operation_id)
);

CREATE FUNCTION runner.registration_receipts_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'registration_receipts is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_registration_receipts_refuse_update
    BEFORE UPDATE ON runner.registration_receipts
    FOR EACH ROW EXECUTE FUNCTION runner.registration_receipts_refuse_update();

GRANT SELECT, INSERT ON runner.registration_receipts TO api_runtime;

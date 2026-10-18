-- The final charge for one chat turn, written once when its attempt settles. Read
-- by the usage export and the billing page. Append-only (rule 9). It hangs off
-- `authoring.attempts`, which keeps opaque owner identifiers, so erasing a draft,
-- user, workspace or tenant never deletes it (deletion matrix).
--
-- `tenant_id` is a scope copy of the attempt's (rule 5's named exception), kept so
-- the billing page can seek a tenant's settlements by index before joining draft
-- and fleet details. Its consistency rule is the trigger below: every insert takes
-- it from the attempt, so no writer can spell it differently.

CREATE TABLE billing.authoring_settlements (
    id                 UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_settlements_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id         UUID   NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    tenant_id          UUID   NOT NULL,
    report_id          UUID   NOT NULL,
    accounting_digest  TEXT   NOT NULL,
    outcome            TEXT   NOT NULL,
    failure_reason     TEXT,
    usage              JSONB  NOT NULL,
    charged_nanos      BIGINT NOT NULL,
    created_at         TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_authoring_settlements_attempt_id UNIQUE (attempt_id)
);

-- The billing page, newest first per tenant.
CREATE INDEX idx_authoring_settlements_tenant_id_created_at_id
    ON billing.authoring_settlements (tenant_id, created_at DESC, id DESC);

CREATE FUNCTION billing.authoring_settlements_scope() RETURNS trigger AS $$
BEGIN
    SELECT a.tenant_id INTO STRICT NEW.tenant_id
      FROM authoring.attempts a
     WHERE a.id = NEW.attempt_id;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_authoring_settlements_scope
    BEFORE INSERT ON billing.authoring_settlements
    FOR EACH ROW EXECUTE FUNCTION billing.authoring_settlements_scope();

CREATE FUNCTION billing.authoring_settlements_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'authoring_settlements is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_authoring_settlements_refuse_update
    BEFORE UPDATE ON billing.authoring_settlements
    FOR EACH ROW EXECUTE FUNCTION billing.authoring_settlements_refuse_update();

GRANT SELECT, INSERT ON billing.authoring_settlements TO api_runtime;

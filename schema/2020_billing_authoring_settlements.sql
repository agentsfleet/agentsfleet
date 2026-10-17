-- The final charge for one chat turn, written once when its attempt settles. Read
-- by the usage export and the billing page. Append-only (rule 9). It hangs off
-- `authoring.attempts`, which keeps opaque owner identifiers, so erasing a draft,
-- user, workspace or tenant never deletes it (deletion matrix).

CREATE TABLE billing.authoring_settlements (
    id                 UUID PRIMARY KEY,
    CONSTRAINT ck_authoring_settlements_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    attempt_id         UUID   NOT NULL REFERENCES authoring.attempts(id) ON DELETE CASCADE,
    report_id          UUID   NOT NULL,
    accounting_digest  TEXT   NOT NULL,
    outcome            TEXT   NOT NULL,
    failure_reason     TEXT,
    usage              JSONB  NOT NULL,
    charged_nanos      BIGINT NOT NULL,
    created_at         BIGINT NOT NULL,
    CONSTRAINT uq_authoring_settlements_attempt_id UNIQUE (attempt_id)
);

CREATE FUNCTION billing.authoring_settlements_refuse_update() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'authoring_settlements is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_authoring_settlements_refuse_update
    BEFORE UPDATE ON billing.authoring_settlements
    FOR EACH ROW EXECUTE FUNCTION billing.authoring_settlements_refuse_update();

GRANT SELECT, INSERT ON billing.authoring_settlements TO api_runtime;

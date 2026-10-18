-- What a piece of billed work may still spend: the built-in biller's
-- reservation, one row per work. `ceiling_nanos` is the most the work has been
-- allowed, `captured_nanos` what its settled lines have taken, and the
-- difference is held against the tenant's balance until `released_at`.
-- `work_type` spellings are application constants (RULE STS). A hold dies with
-- its tenant; it is a promise about a balance, and the balance goes too.

CREATE TABLE billing.holds (
    id              UUID   PRIMARY KEY,
    CONSTRAINT ck_holds_id_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
    work_type       TEXT   NOT NULL,
    work_id         UUID   NOT NULL,
    tenant_id       UUID   NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    ceiling_nanos   BIGINT NOT NULL,
    captured_nanos  BIGINT NOT NULL,
    released_at     TIMESTAMPTZ(3),
    created_at      TIMESTAMPTZ(3) NOT NULL,
    updated_at      TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_holds_work_type_work_id UNIQUE (work_type, work_id),
    CONSTRAINT ck_holds_non_negative CHECK (ceiling_nanos >= 0 AND captured_nanos >= 0)
);

-- A reservation sums the tenant's open holds under its wallet lock.
CREATE INDEX idx_holds_tenant_id_open ON billing.holds (tenant_id) WHERE released_at IS NULL;

GRANT SELECT, INSERT, UPDATE ON billing.holds TO api_runtime;

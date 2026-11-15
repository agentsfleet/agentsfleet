-- Settled runs counted per tenant, UTC day, work type and fleet. A run counts
-- once, on the day of its first runtime line, in the transaction that settles
-- that line: no smaller runtime slice can arrive later, and a line settles once.
CREATE TABLE billing.runs_daily (
    tenant_id   UUID   NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    day         DATE   NOT NULL,
    work_type   TEXT   NOT NULL,
    fleet_id    UUID,
    runs        BIGINT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    updated_at  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_runs_daily_tenant_id_day_work_type_fleet_id
        UNIQUE NULLS NOT DISTINCT (tenant_id, day, work_type, fleet_id),
    CONSTRAINT ck_runs_daily_positive CHECK (runs > 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.runs_daily TO api_runtime;

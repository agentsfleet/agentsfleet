-- Settled runs per tenant, UTC day, work type and fleet. A run counts once, on the day of its
-- first runtime line. Keyed by its natural key because every write is an upsert; no foreign keys,
-- so a settle never fails on an erased owner.
CREATE TABLE billing.runs_daily (
    tenant_id   UUID   NOT NULL,
    day         DATE   NOT NULL,
    work_type   TEXT   NOT NULL,
    fleet_id    UUID,
    run_count   BIGINT NOT NULL,
    created_at  TIMESTAMPTZ(3) NOT NULL,
    updated_at  TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_runs_daily_tenant_id_day_work_type_fleet_id
        UNIQUE NULLS NOT DISTINCT (tenant_id, day, work_type, fleet_id),
    CONSTRAINT ck_runs_daily_positive CHECK (run_count > 0)
);

GRANT SELECT, INSERT, UPDATE ON billing.runs_daily TO api_runtime;

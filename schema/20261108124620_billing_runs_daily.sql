-- Settled runs counted per tenant, UTC day, work type and fleet. A run counts
-- once, on the day of its first runtime line, in the transaction that settles
-- that line: no smaller runtime slice can arrive later, and a line settles once.
-- Runs are not a column of `usage_daily` because one run spans several models
-- and days. Writer: `ledger/sql/count_run.sql`. Reader: `report/sql/aggregates.sql`.
-- No `id` column: the natural key is the row's identity, because every write
-- is an upsert onto it. No foreign keys, as in `usage_lines`: `tenant_id` and
-- `fleet_id` are opaque so a settle never fails on an erased owner.
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

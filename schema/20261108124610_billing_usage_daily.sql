-- Settled usage summed per tenant, UTC day, work type, fleet and model, so a
-- report reads a month as a few rows per fleet instead of every line in it.
-- Writer: `ledger/sql/add_usage_daily.sql`, in the settle transaction, adding
-- exactly the lines that transaction stamps settled. Reader:
-- `report/sql/aggregates.sql`. The ledger stays the source of truth, and this
-- table can be rebuilt from its settled lines.
-- No `id` column: the natural key is the row's identity, because every write
-- is an upsert onto it. No foreign keys, as in `usage_lines`: money outlives
-- every deletion, so `tenant_id` and `fleet_id` are opaque and a settle never
-- fails on an erased owner. `fleet_name` is the name the key's most recent
-- settlement carried.
CREATE TABLE billing.usage_daily (
    tenant_id             UUID   NOT NULL,
    day                   DATE   NOT NULL,
    work_type             TEXT   NOT NULL,
    fleet_id              UUID,
    model                 TEXT   NOT NULL,
    fleet_name            TEXT,
    charged_nanos         BIGINT NOT NULL,
    input_token_count     BIGINT NOT NULL,
    output_token_count    BIGINT NOT NULL,
    runtime_ms            BIGINT NOT NULL,
    line_count            BIGINT NOT NULL,
    unpriced_token_count  BIGINT NOT NULL,
    created_at            TIMESTAMPTZ(3) NOT NULL,
    updated_at            TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_usage_daily_tenant_id_day_work_type_fleet_id_model
        UNIQUE NULLS NOT DISTINCT (tenant_id, day, work_type, fleet_id, model),
    CONSTRAINT ck_usage_daily_non_negative
        CHECK (charged_nanos >= 0 AND input_token_count >= 0 AND output_token_count >= 0
            AND runtime_ms >= 0 AND line_count > 0 AND unpriced_token_count >= 0)
);

GRANT SELECT, INSERT, UPDATE ON billing.usage_daily TO api_runtime;

-- Settled usage summed per tenant, UTC day, work type, fleet and model, so a
-- report reads a month as a few rows per fleet instead of every line in it.
-- The settle transaction adds each line here as it stamps the line settled, so
-- a row holds exactly the settled lines of its key; the ledger stays the source
-- of truth and the table can be rebuilt from it.
CREATE TABLE billing.usage_daily (
    tenant_id        UUID   NOT NULL REFERENCES identity.tenants(id) ON DELETE CASCADE,
    day              DATE   NOT NULL,
    work_type        TEXT   NOT NULL,
    fleet_id         UUID,
    model            TEXT   NOT NULL,
    fleet_name       TEXT,
    charged_nanos    BIGINT NOT NULL,
    input_tokens     BIGINT NOT NULL,
    output_tokens    BIGINT NOT NULL,
    runtime_ms       BIGINT NOT NULL,
    line_count       BIGINT NOT NULL,
    unpriced_tokens  BIGINT NOT NULL,
    created_at       TIMESTAMPTZ(3) NOT NULL,
    updated_at       TIMESTAMPTZ(3) NOT NULL,
    CONSTRAINT uq_usage_daily_tenant_id_day_work_type_fleet_id_model
        UNIQUE NULLS NOT DISTINCT (tenant_id, day, work_type, fleet_id, model),
    CONSTRAINT ck_usage_daily_non_negative
        CHECK (charged_nanos >= 0 AND input_tokens >= 0 AND output_tokens >= 0
            AND runtime_ms >= 0 AND line_count > 0 AND unpriced_tokens >= 0)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON billing.usage_daily TO api_runtime;

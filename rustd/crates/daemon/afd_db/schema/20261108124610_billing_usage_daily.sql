-- Usage per tenant, UTC day, work type, fleet and model: the report's rollup, rebuildable from the
-- lines. Keyed by its natural key because every write is an upsert. No foreign keys: money outlives
-- deletion. `fleet_name` is the name the key's latest line carried.
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

-- A statement's new lines join their rows, kept under the lines' own insert. A line the unique key
-- turned away is not in `inserted`, so it is added once. Rows are written in key order, so two
-- statements touching the same rows take their locks in the same order. The day is the line's UTC
-- day: the function runs in UTC.
CREATE FUNCTION billing.usage_lines_add_daily() RETURNS trigger
    LANGUAGE plpgsql SET search_path TO pg_catalog, billing SET timezone TO UTC
AS $$
BEGIN
    INSERT INTO billing.usage_daily AS d
        (tenant_id, day, work_type, fleet_id, model, fleet_name, charged_nanos, input_token_count,
         output_token_count, runtime_ms, line_count, unpriced_token_count, created_at, updated_at)
    SELECT line.tenant_id, line.created_at::date, line.work_type, line.fleet_id, line.model,
           max(line.fleet_name), sum(line.charged_nanos), sum(line.input_token_count),
           sum(line.output_token_count), sum(line.runtime_ms), count(*),
           sum(line.unpriced_token_count), now(), now()
      FROM inserted line
     GROUP BY line.tenant_id, line.created_at::date, line.work_type, line.fleet_id, line.model
     ORDER BY line.tenant_id, line.created_at::date, line.work_type, line.fleet_id, line.model
    ON CONFLICT ON CONSTRAINT uq_usage_daily_tenant_id_day_work_type_fleet_id_model DO UPDATE
    SET charged_nanos = d.charged_nanos + EXCLUDED.charged_nanos,
        input_token_count = d.input_token_count + EXCLUDED.input_token_count,
        output_token_count = d.output_token_count + EXCLUDED.output_token_count,
        runtime_ms = d.runtime_ms + EXCLUDED.runtime_ms,
        line_count = d.line_count + EXCLUDED.line_count,
        unpriced_token_count = d.unpriced_token_count + EXCLUDED.unpriced_token_count,
        fleet_name = EXCLUDED.fleet_name,
        updated_at = EXCLUDED.updated_at;
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_usage_lines_add_daily
    AFTER INSERT ON billing.usage_lines REFERENCING NEW TABLE AS inserted
    FOR EACH STATEMENT EXECUTE FUNCTION billing.usage_lines_add_daily();

GRANT SELECT, INSERT, UPDATE ON billing.usage_daily TO api_runtime;

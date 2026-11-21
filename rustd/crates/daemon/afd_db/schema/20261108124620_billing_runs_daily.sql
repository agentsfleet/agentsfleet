-- Runs per tenant, UTC day, work type and fleet. A run counts once, on the day of its first runtime
-- slice. Keyed by its natural key because every write is an upsert; no foreign keys, so a write
-- never fails on an erased owner.
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

-- Counts a run when a statement writes its first runtime slice: the work's new line that carries run
-- time (only a runtime slice does; `af_wire::usage::UsageLine`) with the smallest number, and no
-- line of its kind numbered lower already logged. Slices are numbered by their start and written in
-- that order, so no smaller one arrives later. Rows are written in key order, as the usage rollup's.
CREATE FUNCTION billing.usage_lines_count_runs() RETURNS trigger
    LANGUAGE plpgsql SET search_path TO pg_catalog, billing SET timezone TO UTC
AS $$
BEGIN
    INSERT INTO billing.runs_daily AS r
        (tenant_id, day, work_type, fleet_id, run_count, created_at, updated_at)
    SELECT opening.tenant_id, opening.created_at::date, opening.work_type, opening.fleet_id,
           count(*), now(), now()
      FROM (SELECT DISTINCT ON (line.work_type, line.work_id) line.*
              FROM inserted line WHERE line.runtime_ms > 0
             ORDER BY line.work_type, line.work_id, line.seq) opening
     WHERE NOT EXISTS (
           SELECT 1 FROM billing.usage_lines u
            WHERE u.work_type = opening.work_type AND u.work_id = opening.work_id
              AND u.kind = opening.kind AND u.seq < opening.seq)
     GROUP BY opening.tenant_id, opening.created_at::date, opening.work_type, opening.fleet_id
     ORDER BY opening.tenant_id, opening.created_at::date, opening.work_type, opening.fleet_id
    ON CONFLICT ON CONSTRAINT uq_runs_daily_tenant_id_day_work_type_fleet_id DO UPDATE
    SET run_count = r.run_count + EXCLUDED.run_count, updated_at = EXCLUDED.updated_at;
    RETURN NULL;
END;
$$;

CREATE TRIGGER trg_usage_lines_count_runs
    AFTER INSERT ON billing.usage_lines REFERENCING NEW TABLE AS inserted
    FOR EACH STATEMENT EXECUTE FUNCTION billing.usage_lines_count_runs();

GRANT SELECT, INSERT, UPDATE ON billing.runs_daily TO api_runtime;

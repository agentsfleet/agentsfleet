-- Counts the run once, on the UTC day of its first runtime line, when this
-- settlement holds that line: the smallest runtime slice settled here ($6), with
-- no smaller one anywhere in the ledger. Slices are numbered by their start, so
-- no smaller slice arrives later, and a line is stamped settled once.
INSERT INTO billing.runs_daily AS r (tenant_id, day, work_type, fleet_id, run_count, created_at, updated_at)
SELECT $3, ($7::timestamptz AT TIME ZONE 'UTC')::date, $1, $4, 1, now(), now()
WHERE NOT EXISTS (
    SELECT 1 FROM billing.usage_lines u
    WHERE u.work_type = $1 AND u.work_id = $2 AND u.kind = $5 AND u.seq < $6
)
ON CONFLICT ON CONSTRAINT uq_runs_daily_tenant_id_day_work_type_fleet_id DO UPDATE
SET run_count = r.run_count + 1, updated_at = EXCLUDED.updated_at

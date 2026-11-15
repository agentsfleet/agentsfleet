-- Adds the lines this settlement just stamped settled into their tenant, UTC day,
-- work type, fleet and model rows. Rows are written in key order, so two
-- settlements touching the same rows take their locks in the same order.
INSERT INTO billing.usage_daily AS d
    (tenant_id, day, work_type, fleet_id, model, fleet_name, charged_nanos, input_tokens,
     output_tokens, runtime_ms, line_count, unpriced_tokens, created_at, updated_at)
SELECT line.tenant_id, (line.created_at AT TIME ZONE 'UTC')::date, $1, line.fleet_id, line.model,
       MAX(line.fleet_name), SUM(line.charged), SUM(line.input), SUM(line.output),
       SUM(line.runtime_ms), COUNT(*), SUM(line.unpriced), now(), now()
FROM unnest($2::uuid[], $3::uuid[], $4::text[], $5::text[], $6::timestamptz[], $7::bigint[],
            $8::bigint[], $9::bigint[], $10::bigint[], $11::bigint[])
    AS line(tenant_id, fleet_id, fleet_name, model, created_at, charged, input, output,
            runtime_ms, unpriced)
GROUP BY line.tenant_id, (line.created_at AT TIME ZONE 'UTC')::date, line.fleet_id, line.model
ORDER BY line.tenant_id, (line.created_at AT TIME ZONE 'UTC')::date, line.fleet_id, line.model
ON CONFLICT ON CONSTRAINT uq_usage_daily_tenant_id_day_work_type_fleet_id_model DO UPDATE
SET charged_nanos = d.charged_nanos + EXCLUDED.charged_nanos,
    input_tokens = d.input_tokens + EXCLUDED.input_tokens,
    output_tokens = d.output_tokens + EXCLUDED.output_tokens,
    runtime_ms = d.runtime_ms + EXCLUDED.runtime_ms,
    line_count = d.line_count + EXCLUDED.line_count,
    unpriced_tokens = d.unpriced_tokens + EXCLUDED.unpriced_tokens,
    fleet_name = EXCLUDED.fleet_name,
    updated_at = EXCLUDED.updated_at

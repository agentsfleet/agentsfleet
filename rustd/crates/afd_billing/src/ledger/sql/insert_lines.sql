INSERT INTO billing.usage_lines
    (id, work_type, work_id, seq, tenant_id, workspace_id, fleet_id, fleet_name, event_id,
     authoring_attempt_id, provider, model, posture, requested_at, input_token_count,
     cached_input_token_count, cache_write_token_count, cache_write_5m_token_count,
     cache_write_1h_token_count, output_token_count, runtime_ms, platform_model_price_id,
     unpriced_token_count, charged_nanos, created_at, updated_at)
SELECT line.id, $2, $3::uuid, line.seq, $4::uuid, $5::uuid, $6::uuid, $7, $8, $9::uuid, $10, $11,
       $12, line.requested_at, line.input, line.cached, line.write, line.write_5m, line.write_1h,
       line.output, line.runtime_ms, line.price_id, line.unpriced, line.charged, now(), now()
FROM unnest($1::text[]::uuid[], $13::bigint[], $14::timestamptz[], $15::bigint[], $16::bigint[],
            $17::bigint[], $18::bigint[], $19::bigint[], $20::bigint[], $21::bigint[],
            $22::text[]::uuid[], $23::bigint[], $24::bigint[])
    AS line(id, seq, requested_at, input, cached, write, write_5m, write_1h, output, runtime_ms,
            price_id, unpriced, charged)
ON CONFLICT (work_type, work_id, seq) DO NOTHING

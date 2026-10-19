SELECT work_type, work_id::text AS work_id, seq, tenant_id::text AS tenant_id,
       workspace_id::text AS workspace_id, fleet_id::text AS fleet_id, fleet_name, event_id,
       provider, model, posture, requested_at, input_token_count, cached_input_token_count,
       cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
       output_token_count, runtime_ms, platform_model_price_id::text AS price_id,
       unpriced_token_count, charged_nanos
FROM billing.usage_lines
WHERE settled_at IS NULL AND created_at < now() - $1::interval
ORDER BY created_at, id
LIMIT $2

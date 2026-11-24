SELECT id, tenant_id, workspace_id, fleet_id, fleet_name, turn_id, run_id,
       $3::text AS charge_type, posture, model, charged_nanos, input_token_count,
       output_token_count, runtime_ms AS wall_ms, created_at
  FROM billing.usage_lines
 WHERE tenant_id = $1
 ORDER BY created_at DESC, id DESC
 LIMIT $2

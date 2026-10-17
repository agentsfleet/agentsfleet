SELECT id::text, tenant_id::text, workspace_id::text, fleet_id::text, fleet_name,
       event_id, charge_type, posture, model, charged_nanos,
       input_token_count, output_token_count, wall_ms, created_at
FROM billing.fleet_charges
WHERE tenant_id = $1::uuid
ORDER BY created_at DESC, fleet_charges.id DESC
LIMIT $2

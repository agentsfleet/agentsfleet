INSERT INTO billing.fleet_charges
  (id, tenant_id, workspace_id, fleet_id, event_id,
   charge_type, posture, model,
   charged_nanos,
   input_token_count, cached_input_token_count, output_token_count, wall_ms,
   event_created_at, created_at, updated_at, fleet_name)
VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5, $6, $7, $8, $9, $10,
        $11, $12, $13, $14, now(), now(),
        (SELECT f.name FROM fleet.fleets f WHERE f.id = $4::uuid))
ON CONFLICT (event_id, charge_type, fleet_id) DO NOTHING

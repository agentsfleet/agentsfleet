WITH raised AS (
  INSERT INTO fleet.approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name,
     gate_type, proposed_action, evidence, blast_radius, expires_at,
     status, detail, created_at, updated_at, event_id, stated_binding,
     spend_count, spend_ceiling)
  VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9::jsonb, $10,
          now() + $11::interval, $12, '', now(), now(), $13, $14::jsonb, $15, $16)
  RETURNING id, fleet_id
)
SELECT COUNT(*) FROM (
  SELECT g.id FROM fleet.approval_gates g, raised
   WHERE g.fleet_id = raised.fleet_id AND g.resolved_at IS NULL
  UNION ALL SELECT id FROM raised
) pending

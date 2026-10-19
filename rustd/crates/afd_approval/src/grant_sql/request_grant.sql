WITH requested AS (
  SELECT id, status FROM connector.integration_grants
  WHERE fleet_id = $1::uuid AND service = $2
  FOR UPDATE
), raised AS (
  INSERT INTO fleet.approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name,
     gate_type, proposed_action, evidence, blast_radius, expires_at,
     status, detail, created_at, updated_at, event_id, stated_binding,
     spend_count, spend_ceiling, active_grant_id)
  SELECT $4::uuid, $1::uuid, $5::uuid, $6, $7, $8,
         $9, $10, $11::jsonb, $12, now() + $13::interval,
         $14, '', now(), now(), NULL, NULL, NULL, NULL, requested.id
  FROM requested
  WHERE requested.status = $3 AND NOT EXISTS (
    SELECT 1 FROM fleet.approval_gates g
    WHERE g.active_grant_id = requested.id
  )
  ON CONFLICT (active_grant_id) DO NOTHING
  RETURNING id
)
SELECT (SELECT COUNT(*) FROM raised) AS raised, requested.status AS found
FROM requested

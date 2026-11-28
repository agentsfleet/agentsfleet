-- Asks a person for one grant, on the row ENSURE_GRANT left in place. A revoked grant is
-- asked again and moves back to pending; a pending grant with no open card, or an approved
-- one asked for a scope it does not hold, gets a card; an open card stands and none is
-- raised beside it. The asked scope waits in `requested_scope`, so an approved scope stays
-- in force until a person approves the card.
WITH requested AS MATERIALIZED (
  SELECT id, status, scope FROM connector.integration_grants
  WHERE fleet_id = $1 AND grant_type = $3 AND service = $2
  FOR UPDATE
), open_card AS MATERIALIZED (
  SELECT g.id FROM fleet.approval_gates g
  JOIN requested r ON g.active_grant_id = r.id
), asked AS MATERIALIZED (
  SELECT r.id, r.status FROM requested r
  WHERE r.status = $6
     OR (NOT EXISTS (SELECT 1 FROM open_card)
         AND (r.status = $5
              OR ($4::jsonb IS NOT NULL AND r.scope IS DISTINCT FROM $4::jsonb)))
), reopened AS (
  UPDATE connector.integration_grants g
  SET status          = CASE WHEN a.status = $6 THEN $5 ELSE g.status END,
      requested_scope = CASE WHEN EXISTS (SELECT 1 FROM open_card) THEN g.requested_scope
                             ELSE $4::jsonb END,
      approved_at     = CASE WHEN a.status = $6 THEN NULL ELSE g.approved_at END,
      revoked_at      = CASE WHEN a.status = $6 THEN NULL ELSE g.revoked_at END,
      updated_at      = now()
  FROM asked a
  WHERE g.id = a.id
  RETURNING g.id
), raised AS (
  INSERT INTO fleet.approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name,
     gate_type, proposed_action, evidence, blast_radius, expires_at,
     status, detail, created_at, updated_at, turn_id, active_grant_id)
  SELECT $7, $1, $8, $9, $2, $2,
         $11, $12, $13::jsonb, $14, clock_timestamp() + $15::interval,
         $16, '', now(), now(), $10, reopened.id
  FROM reopened
  WHERE NOT EXISTS (SELECT 1 FROM open_card)
  ON CONFLICT (active_grant_id) DO NOTHING
  RETURNING id
)
SELECT r.id AS grant_id,
       EXISTS (SELECT 1 FROM asked) AS asked,
       (SELECT id FROM raised) AS raised,
       (SELECT id FROM open_card) AS open
FROM requested r

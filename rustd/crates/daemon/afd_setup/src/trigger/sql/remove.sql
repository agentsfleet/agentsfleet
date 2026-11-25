-- Removes one trigger of one fleet; the workspace predicate keeps a person to their own.
--
-- $1 trigger · $2 fleet · $3 workspace
DELETE FROM fleet.triggers
 WHERE id = $1 AND fleet_id = $2 AND workspace_id = $3
RETURNING trigger_type

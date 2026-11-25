-- Whether the workspace holds the fleet, locking nothing.
--
-- $1 fleet · $2 workspace
SELECT id FROM fleet.fleets WHERE id = $1 AND workspace_id = $2

-- One fleet of a workspace by its name, in the columns a list page reads, so
-- the list route answers a name with the row it would have listed.
-- `uq_fleets_workspace_id_name` makes it at most one row and serves the lookup.
--
-- $1 workspace · $2 name
SELECT f.id AS id,
       f.name,
       f.status,
       f.created_at,
       f.updated_at,
       COALESCE(c.event_count, 0) AS event_count,
       COALESCE(c.budget_used_nanos, 0) AS budget_used_nanos
FROM fleet.fleets f
LEFT JOIN fleet.activity_counters c ON c.fleet_id = f.id
WHERE f.workspace_id = $1
  AND f.name = $2

-- The fleets of a workspace subscribed to one chat channel, with the saved name a person
-- addresses each by and the GitHub scope a person approved for it, which decides whether an
-- unaddressed mention may reach it.
--
-- $1 workspace · $2 the statuses a subscriber can be read in · $3 the mention kind ·
-- $4 provider · $5 channel · $6 the GitHub grant's name · $7 the approved status
SELECT f.id AS fleet_id,
       f.name,
       f.status,
       g.scope::text AS github_scope
FROM fleet.triggers t
JOIN fleet.fleets f ON f.id = t.fleet_id AND f.workspace_id = t.workspace_id
LEFT JOIN connector.integration_grants g
       ON g.fleet_id = f.id AND g.service = $6 AND g.status = $7
WHERE t.workspace_id = $1
  AND f.status = ANY($2::text[])
  AND t.kind = $3
  AND t.source = $4
  AND t.channel = $5
ORDER BY f.id

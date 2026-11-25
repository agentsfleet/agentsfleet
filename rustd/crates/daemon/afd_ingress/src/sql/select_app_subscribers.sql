-- The active fleets of one workspace subscribed to a provider's App events, each with its row.
--
-- $1 workspace · $2 the status a fleet must hold · $3 the App event trigger type · $4 provider
SELECT f.id AS fleet_id,
       f.workspace_id AS workspace_id,
       f.status,
       t.source,
       t.events,
       t.repositories,
       t.verifier_scheme,
       t.verifier_header,
       t.secret_name
FROM fleet.triggers t
JOIN fleet.fleets f ON f.id = t.fleet_id AND f.workspace_id = t.workspace_id
WHERE t.workspace_id = $1
  AND f.status = $2
  AND t.trigger_type = $3
  AND t.source = $4
ORDER BY f.id

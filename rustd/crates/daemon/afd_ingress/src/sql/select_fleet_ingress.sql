-- The fleet a signed webhook delivery is addressed to, and its row for the URL's source. The
-- trigger's workspace is the fleet's, so a row never routes outside the workspace that owns it.
--
-- $1 fleet · $2 the webhook kind · $3 source
SELECT f.workspace_id AS workspace_id,
       f.status,
       t.source,
       t.events,
       t.repositories,
       t.verifier_scheme,
       t.verifier_header,
       t.secret_name
FROM fleet.fleets f
JOIN fleet.triggers t ON t.fleet_id = f.id AND t.workspace_id = f.workspace_id
WHERE f.id = $1
  AND t.kind = $2
  AND t.source = $3

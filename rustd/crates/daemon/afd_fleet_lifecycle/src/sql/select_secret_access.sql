-- Which runnable fleets in a workspace may read each secret, by name.
-- A run resolves exactly the names its fleet's approved grants carry, so those
-- grants are the whole answer. A pending request, a revoked grant, a trigger's
-- signing secret and a textual reference grant nothing here.
--
-- $1 workspace · $2 the runnable fleet status · $3 the approved grant status
SELECT g.service AS secret_name, f.id AS fleet_id, f.name AS fleet_name
  FROM fleet.fleets f
  JOIN connector.integration_grants g ON g.fleet_id = f.id
 WHERE f.workspace_id = $1
   AND f.status = $2
   AND g.status = $3
 ORDER BY secret_name, fleet_name, fleet_id

-- The grants a person approved for one fleet, by name, each with the channel it reaches a run
-- through and its approved scope. Pending and revoked rows grant nothing, so they are not read.
--
-- $1 fleet · $2 the approved status
SELECT service AS name, grant_type, scope::text AS scope
FROM connector.integration_grants
WHERE fleet_id = $1 AND status = $2
ORDER BY service
